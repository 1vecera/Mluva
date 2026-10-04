//! Pinned native ONNX/CUDA libraries; a wheel is an archive, never an interpreter dependency.
use crate::{ProviderError, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::{
    fs,
    io::{Read, Write},
    os::unix::fs::{OpenOptionsExt, PermissionsExt},
    path::{Component, Path, PathBuf},
    sync::LazyLock,
};
use tokio_util::sync::CancellationToken;

pub const GPU_RUNTIME_BUDGET: u64 = 3_500_000_000;
pub const GPU_FREE_SPACE: u64 = 6_000_000_000;
pub const CPU_RUNTIME_BUDGET: u64 = 50_000_000;
const INVALID: &str = "Runtime verification failed. Download again.";
const IO_FAILURE: &str = "Local model files could not be read or written.";

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeMember {
    pub member: String,
    pub path: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct NativeArchive {
    pub package: String,
    pub version: String,
    pub url: String,
    pub size: u64,
    pub sha256: String,
    pub members: Vec<NativeMember>,
}
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct OnnxManifest {
    pub cpu: Vec<NativeArchive>,
    pub cuda: Vec<NativeArchive>,
}
pub static ONNX_RUNTIME: LazyLock<OnnxManifest> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/onnx-runtime.json"))
        .expect("compiled native ONNX runtime manifest")
});

impl OnnxManifest {
    pub fn archives(&self, device: &str) -> Result<&[NativeArchive]> {
        match device {
            "cpu" => Ok(&self.cpu),
            "cuda" => Ok(&self.cuda),
            _ => Err(ProviderError::message("Unknown local runtime device.")),
        }
    }
    pub fn root(&self, data: &Path, device: &str) -> PathBuf {
        if device == "cuda" {
            data.join("gpu-runtime")
        } else {
            data.join("onnx-runtime/cpu")
        }
    }
    pub fn library(&self, data: &Path, device: &str) -> Result<PathBuf> {
        let archive = self
            .archives(device)?
            .first()
            .ok_or_else(|| ProviderError::message(INVALID))?;
        let member = archive
            .members
            .iter()
            .find(|member| member.path.starts_with("lib/libonnxruntime.so."))
            .ok_or_else(|| ProviderError::message(INVALID))?;
        Ok(self.root(data, device).join(&member.path))
    }
    pub fn stamp(&self, device: &str) -> Result<String> {
        let bytes = serde_json::to_vec(self.archives(device)?)
            .map_err(|_| ProviderError::message(INVALID))?;
        Ok(Sha256::digest(bytes)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect())
    }
    pub fn ready(&self, data: &Path, device: &str) -> bool {
        let Ok(archives) = self.archives(device) else {
            return false;
        };
        if archives.is_empty() || archives.iter().any(|archive| archive.members.is_empty()) {
            return false;
        }
        let Ok(stamp) = self.stamp(device) else {
            return false;
        };
        let root = self.root(data, device);
        fs::read_to_string(root.join(".native-ready")).is_ok_and(|value| value == stamp)
            && archives
                .iter()
                .flat_map(|archive| &archive.members)
                .all(|member| {
                    fs::metadata(root.join(&member.path))
                        .is_ok_and(|metadata| metadata.is_file() && metadata.len() == member.size)
                })
    }
}

/// Detect an NVIDIA device without loading a model or requiring a Python package manager.
pub async fn gpu_name() -> &'static str {
    static NAME: tokio::sync::OnceCell<String> = tokio::sync::OnceCell::const_new();
    NAME.get_or_init(|| async {
        if std::env::consts::ARCH != "x86_64" {
            return String::new();
        }
        let mut command = tokio::process::Command::new("nvidia-smi");
        command
            .args(["--query-gpu=name", "--format=csv,noheader", "--id=0"])
            .env_clear()
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::null())
            .kill_on_drop(true);
        for key in ["PATH", "LANG", "SYSTEMROOT"] {
            if let Some(value) = std::env::var_os(key) {
                command.env(key, value);
            }
        }
        let Ok(mut child) = command.spawn() else {
            return String::new();
        };
        let mut output = child.stdout.take().unwrap();
        use tokio::io::AsyncReadExt;
        let outcome = tokio::time::timeout(std::time::Duration::from_secs(2), async {
            let mut bytes = Vec::new();
            (&mut output)
                .take(65_537)
                .read_to_end(&mut bytes)
                .await
                .ok()?;
            if bytes.len() > 65_536 {
                return None;
            }
            let status = child.wait().await.ok()?;
            if !status.success() {
                return None;
            }
            let value = String::from_utf8(bytes).ok()?;
            Some(
                value
                    .trim_matches(mluva_core::text::whitespace)
                    .split([
                        '\n', '\r', '\u{000b}', '\u{000c}', '\u{001c}', '\u{001d}', '\u{001e}',
                        '\u{0085}', '\u{2028}', '\u{2029}',
                    ])
                    .next()?
                    .to_owned(),
            )
        })
        .await;
        if child.try_wait().ok().flatten().is_none() {
            let _ = child.kill().await;
        }
        let _ = child.wait().await;
        outcome.ok().flatten().unwrap_or_default()
    })
    .await
}

pub fn check_gpu_storage(used: u64, free: u64) -> Result<()> {
    if used.saturating_add(GPU_RUNTIME_BUDGET) > crate::local_assets::STORAGE_LIMIT {
        return Err(ProviderError::message(
            "GPU support needs 3.5 GB of the 5 GB local storage budget. Remove unused models first.",
        ));
    }
    if free < GPU_FREE_SPACE {
        return Err(ProviderError::message(
            "GPU installation needs 6 GB free temporarily to unpack its wheels.",
        ));
    }
    Ok(())
}

/// Extract only named native libraries/notices, checking every member's size/SHA and path.
pub(crate) fn extract(
    archive: &Path,
    root: &Path,
    files: &[NativeMember],
    cancelled: &CancellationToken,
) -> Result<()> {
    let invalid = || ProviderError::message(INVALID);
    let io = || ProviderError::message(IO_FAILURE);
    let mut archive =
        zip::ZipArchive::new(fs::File::open(archive).map_err(|_| io())?).map_err(|_| invalid())?;
    let mut seen = std::collections::HashSet::new();
    for spec in files {
        if cancelled.is_cancelled() {
            return Err(ProviderError::message("Download cancelled."));
        }
        if !seen.insert(&spec.path) || !safe_relative(&spec.path) || !safe_relative(&spec.member) {
            return Err(invalid());
        }
        let mut member = archive.by_name(&spec.member).map_err(|_| invalid())?;
        if member.size() != spec.size
            || member.is_dir()
            || member.unix_mode().is_some_and(|mode| {
                mode & libc::S_IFMT != 0 && mode & libc::S_IFMT != libc::S_IFREG
            })
        {
            return Err(invalid());
        }
        let target = root.join(&spec.path);
        fs::DirBuilder::new()
            .recursive(true)
            .create(target.parent().unwrap())
            .map_err(|_| io())?;
        let mut target = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .custom_flags(libc::O_NOFOLLOW)
            .open(&target)
            .map_err(|_| io())?;
        let mut digest = Sha256::new();
        let mut count = 0_u64;
        let mut buffer = vec![0; 1_048_576];
        loop {
            if cancelled.is_cancelled() {
                return Err(ProviderError::message("Download cancelled."));
            }
            let length = member.read(&mut buffer).map_err(|_| invalid())?;
            if length == 0 {
                break;
            }
            count = count.saturating_add(length as u64);
            if count > spec.size {
                return Err(invalid());
            }
            digest.update(&buffer[..length]);
            target.write_all(&buffer[..length]).map_err(|_| io())?;
        }
        let actual: String = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect();
        if count != spec.size || actual != spec.sha256 {
            return Err(invalid());
        }
        target
            .set_permissions(fs::Permissions::from_mode(0o600))
            .map_err(|_| io())?;
    }
    Ok(())
}
fn safe_relative(path: &str) -> bool {
    !path.is_empty()
        && Path::new(path)
            .components()
            .all(|component| matches!(component, Component::Normal(_)))
}
