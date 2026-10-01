//! Pinned, application-owned local model files and Qwen runtimes.
use crate::{ProviderError, Result, USER_AGENT};
use futures_util::StreamExt;
use mluva_core::private_files::resolve_path;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::BTreeMap;
use std::ffi::CString;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{DirBuilderExt, OpenOptionsExt, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::{Arc, LazyLock};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

const BLOCK: usize = 1_048_576;
pub const STORAGE_LIMIT: u64 = 5_000_000_000;
pub const QWEN_RUNTIME_BUDGET: u64 = 250_000_000;
const IO_FAILURE: &str = "Local model files could not be read or written.";
const ARCHIVE_FAILURE: &str = "The runtime archive is invalid or contains an unsafe path.";
const CANCELLED: &str = "Download cancelled.";

fn offered() -> bool {
    true
}
fn is_offered(value: &bool) -> bool {
    *value
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelFile {
    pub name: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModelSpec {
    pub id: String,
    pub label: String,
    pub repo: String,
    pub revision: String,
    pub quantization: String,
    pub engine: String,
    pub ram_mb: u64,
    pub files: Vec<ModelFile>,
    #[serde(default = "offered", skip_serializing_if = "is_offered")]
    pub offered: bool,
}
pub static MODEL_CATALOG: LazyLock<Vec<ModelSpec>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/local-models.json"))
        .expect("compiled local model catalog")
});
pub fn model(identifier: &str) -> Result<&'static ModelSpec> {
    MODEL_CATALOG
        .iter()
        .find(|model| model.id == identifier)
        .ok_or_else(|| ProviderError::message("Unknown local model."))
}
impl ModelSpec {
    pub fn path(&self, data_dir: &Path) -> PathBuf {
        data_dir.join("models").join(format!(
            "{}-{}",
            self.id,
            self.revision.chars().take(12).collect::<String>()
        ))
    }
    pub fn bytes(&self) -> u64 {
        self.files.iter().map(|file| file.size).sum()
    }
    pub fn ready(&self, data_dir: &Path) -> bool {
        let path = self.path(data_dir);
        fs::read_to_string(path.join(".ready")).is_ok_and(|stamp| stamp == self.revision)
            && self.files.iter().all(|file| {
                fs::metadata(path.join(&file.name))
                    .is_ok_and(|metadata| metadata.len() == file.size)
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct RuntimeAsset {
    pub url: String,
    pub size: u64,
    pub sha256: String,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct QwenManifest {
    pub version: String,
    pub assets: BTreeMap<String, RuntimeAsset>,
}
pub static QWEN_RUNTIME: LazyLock<QwenManifest> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/qwen-runtime.json"))
        .expect("compiled Qwen runtime manifest")
});
impl QwenManifest {
    pub fn binary(&self, data_dir: &Path, device: &str) -> PathBuf {
        data_dir
            .join("qwen-runtime")
            .join(device)
            .join(format!("llama-{}", self.version))
            .join("llama-server")
    }
    pub fn ready(&self, data_dir: &Path, device: &str) -> bool {
        let Some(asset) = self.assets.get(device) else {
            return false;
        };
        let root = data_dir.join("qwen-runtime").join(device);
        fs::read_to_string(root.join(".ready")).is_ok_and(|stamp| stamp == asset.sha256)
            && CString::new(self.binary(data_dir, device).as_os_str().as_bytes())
                .is_ok_and(|path| unsafe { libc::access(path.as_ptr(), libc::X_OK) } == 0)
    }
}

pub fn disk_usage(path: &Path) -> u64 {
    let Ok(entries) = fs::read_dir(path) else {
        return 0;
    };
    entries
        .filter_map(std::result::Result::ok)
        .map(|entry| {
            let Ok(metadata) = entry.path().symlink_metadata() else {
                return 0;
            };
            if metadata.file_type().is_symlink() {
                0
            } else if metadata.is_file() {
                metadata.len()
            } else if metadata.is_dir() {
                disk_usage(&entry.path())
            } else {
                0
            }
        })
        .sum()
}
fn free_space(path: &Path) -> Result<u64> {
    let path = CString::new(path.as_os_str().as_bytes())
        .map_err(|_| ProviderError::message(IO_FAILURE))?;
    let mut status = std::mem::MaybeUninit::<libc::statvfs>::uninit();
    if unsafe { libc::statvfs(path.as_ptr(), status.as_mut_ptr()) } != 0 {
        return Err(ProviderError::message(IO_FAILURE));
    }
    let status = unsafe { status.assume_init() };
    Ok(status.f_bavail.saturating_mul(status.f_frsize))
}
pub fn check_model_storage(used: u64, total: u64, free: u64) -> Result<()> {
    if used.saturating_add(total) > STORAGE_LIMIT || free < total.saturating_add(100_000_000) {
        return Err(ProviderError::message(
            "Not enough model storage. Free disk space before downloading.",
        ));
    }
    Ok(())
}
pub fn check_runtime_storage(used: u64, free: u64) -> Result<()> {
    if used.saturating_add(QWEN_RUNTIME_BUDGET) > STORAGE_LIMIT || free < QWEN_RUNTIME_BUDGET {
        return Err(ProviderError::message(
            "Not enough local model storage. Remove unused downloads before continuing.",
        ));
    }
    Ok(())
}
fn private_directory(path: &Path) -> Result<()> {
    fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(path)
        .map_err(|_| ProviderError::message(IO_FAILURE))
}

#[derive(Clone)]
pub struct AssetStore {
    data_dir: PathBuf,
    client: reqwest::Client,
}
impl AssetStore {
    pub fn new(data_dir: &Path) -> Result<Self> {
        let client = reqwest::Client::builder()
            .connect_timeout(Duration::from_secs(30))
            .read_timeout(Duration::from_secs(30))
            .user_agent(USER_AGENT)
            .build()
            .map_err(|_| ProviderError::message("Local model downloads could not connect."))?;
        Ok(Self {
            data_dir: data_dir.into(),
            client,
        })
    }
    /// Own the released nonblocking lock across runtime preparation and weights.
    pub fn begin_download(&self) -> Result<ModelDownload> {
        let root = self.data_dir.join("models");
        private_directory(&root)?;
        let lock = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(root.join(".download.lock"))
            .map_err(|_| ProviderError::message(IO_FAILURE))?;
        use std::os::fd::AsRawFd;
        if unsafe { libc::flock(lock.as_raw_fd(), libc::LOCK_EX | libc::LOCK_NB) } != 0 {
            return Err(ProviderError::message(
                "Another model download is running. Try again when it finishes.",
            ));
        }
        Ok(ModelDownload {
            store: self.clone(),
            _lock: Arc::new(lock),
        })
    }
}
pub struct ModelDownload {
    store: AssetStore,
    _lock: Arc<File>,
}
struct DownloadPolicy<'a> {
    size: u64,
    checksum: Option<&'a str>,
    too_large: &'static str,
    invalid: &'static str,
}
struct Cleanup {
    path: PathBuf,
    armed: bool,
}
impl Drop for Cleanup {
    fn drop(&mut self) {
        if self.armed {
            let _ = fs::remove_dir_all(&self.path);
        }
    }
}
struct ExtractionOwner {
    // Field drop order removes staging before releasing the cross-process lock.
    cleanup: Cleanup,
    _lock: Arc<File>,
}
struct ExtractionCancellation(CancellationToken);
impl Drop for ExtractionCancellation {
    fn drop(&mut self) {
        self.0.cancel();
    }
}
impl ModelDownload {
    fn owned_bytes(&self) -> u64 {
        [
            "models",
            "gpu-runtime",
            "qwen-runtime",
            "qwen-cache",
            "onnx-runtime",
        ]
        .iter()
        .map(|name| disk_usage(&self.store.data_dir.join(name)))
        .sum()
    }
    /// Install pinned weights after the caller prepares the selected inference runtime.
    /// Keep this download guard alive across both phases to exclude other processes.
    pub async fn install_model_files(
        &mut self,
        spec: &ModelSpec,
        cancelled: &CancellationToken,
        progress: &mut (dyn FnMut(f64) + Send),
    ) -> Result<()> {
        if spec.ready(&self.store.data_dir) {
            progress(1.0);
            return Ok(());
        }
        let total = spec.bytes();
        check_model_storage(
            self.owned_bytes(),
            total,
            free_space(&self.store.data_dir.join("models"))?,
        )?;
        let path = spec.path(&self.store.data_dir);
        private_directory(&path)?;
        let mut cleanup = Cleanup {
            path: path.clone(),
            armed: true,
        };
        let mut completed = 0;
        for file in &spec.files {
            let destination = path.join(&file.name);
            if let Some(parent) = destination.parent() {
                fs::create_dir_all(parent).map_err(|_| ProviderError::message(IO_FAILURE))?;
            }
            let temporary = part_path(&destination);
            let url = format!(
                "https://huggingface.co/{}/resolve/{}/{}",
                spec.repo, spec.revision, file.name
            );
            let mut seen = |bytes| progress((completed + bytes) as f64 / total as f64);
            self.download(
                &url,
                &temporary,
                DownloadPolicy {
                    size: file.size,
                    checksum: (!file.sha256.is_empty()).then_some(file.sha256.as_str()),
                    too_large: "The model download exceeded its expected size.",
                    invalid: "Model verification failed. Please retry the download.",
                },
                cancelled,
                &mut seen,
            )
            .await?;
            fs::rename(&temporary, &destination).map_err(|_| ProviderError::message(IO_FAILURE))?;
            completed += file.size;
        }
        if cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        fs::write(path.join(".ready"), &spec.revision)
            .map_err(|_| ProviderError::message(IO_FAILURE))?;
        cleanup.armed = false;
        Ok(())
    }
    pub async fn install_qwen_runtime(
        &mut self,
        manifest: &QwenManifest,
        device: &str,
        cancelled: &CancellationToken,
    ) -> Result<()> {
        if manifest.ready(&self.store.data_dir, device) {
            return Ok(());
        }
        if std::env::consts::ARCH != "x86_64" {
            return Err(ProviderError::message(
                "This Qwen runtime requires Linux x86-64. Choose another local model on this computer.",
            ));
        }
        let root = self.store.data_dir.join("qwen-runtime");
        private_directory(&root)?;
        check_runtime_storage(self.owned_bytes(), free_space(&root)?)?;
        let asset = manifest
            .assets
            .get(device)
            .ok_or_else(|| ProviderError::message("Unknown local runtime device."))?;
        let stage = root.join(format!("{device}.partial"));
        let _ = fs::remove_dir_all(&stage);
        private_directory(&stage)?;
        let mut cleanup = Cleanup {
            path: stage.clone(),
            armed: true,
        };
        let archive = stage.join("runtime.tar.gz");
        self.download(
            &asset.url,
            &archive,
            DownloadPolicy {
                size: asset.size,
                checksum: Some(&asset.sha256),
                too_large: "Runtime exceeds its declared size.",
                invalid: "Runtime verification failed. Download again.",
            },
            cancelled,
            &mut |_| {},
        )
        .await?;
        unpack_runtime(&archive, &stage, QWEN_RUNTIME_BUDGET)?;
        fs::remove_file(archive).map_err(|_| ProviderError::message(IO_FAILURE))?;
        if cancelled.is_cancelled() {
            return Err(ProviderError::message(CANCELLED));
        }
        fs::write(stage.join(".ready"), &asset.sha256)
            .map_err(|_| ProviderError::message(IO_FAILURE))?;
        let installed = root.join(device);
        let _ = fs::remove_dir_all(&installed);
        fs::rename(&stage, &installed).map_err(|_| ProviderError::message(IO_FAILURE))?;
        cleanup.armed = false;
        Ok(())
    }
    /// Prepare native ONNX libraries before weights while retaining this model-download lock.
    pub async fn install_onnx_runtime(
        &mut self,
        manifest: &crate::onnx_runtime::OnnxManifest,
        device: &str,
        cancelled: &CancellationToken,
    ) -> Result<()> {
        use crate::onnx_runtime::{CPU_RUNTIME_BUDGET, GPU_RUNTIME_BUDGET, check_gpu_storage};
        if manifest.ready(&self.store.data_dir, device) {
            // Recover a crash after promotion, under the same exclusive download lock.
            let installed = manifest.root(&self.store.data_dir, device);
            let _ = fs::remove_dir_all(installed.with_extension("partial"));
            let _ = fs::remove_dir_all(installed.with_extension("previous"));
            return Ok(());
        }
        if std::env::consts::ARCH != "x86_64" {
            return Err(ProviderError::message(
                "This native ONNX runtime requires Linux x86-64.",
            ));
        }
        if device == "cuda" && crate::onnx_runtime::gpu_name().await.is_empty() {
            return Err(ProviderError::message(
                "No supported NVIDIA GPU found. Use CPU on this computer.",
            ));
        }
        let archives = manifest.archives(device)?;
        let installed = manifest.root(&self.store.data_dir, device);
        let parent = installed.parent().unwrap();
        private_directory(parent)?;
        let budget = if device == "cuda" {
            GPU_RUNTIME_BUDGET
        } else {
            CPU_RUNTIME_BUDGET
        };
        let budget_failure = if device == "cuda" {
            "GPU runtime exceeds the storage budget. Use CPU."
        } else {
            "Runtime exceeds its storage budget."
        };
        let selected = archives
            .iter()
            .flat_map(|archive| &archive.members)
            .try_fold(0_u64, |total, member| total.checked_add(member.size))
            .ok_or_else(|| ProviderError::message(budget_failure))?;
        if selected > budget {
            return Err(ProviderError::message(budget_failure));
        }
        if archives.is_empty() || archives.iter().any(|archive| archive.members.is_empty()) {
            return Err(ProviderError::message(
                "Runtime verification failed. Download again.",
            ));
        }
        if device == "cuda" {
            let used = ["models", "qwen-runtime", "qwen-cache", "onnx-runtime"]
                .iter()
                .map(|name| disk_usage(&self.store.data_dir.join(name)))
                .sum();
            check_gpu_storage(used, free_space(parent)?)?;
        } else if self.owned_bytes().saturating_add(budget) > STORAGE_LIMIT
            || free_space(parent)? < budget
        {
            return Err(ProviderError::message(
                "Not enough local model storage. Remove unused downloads before continuing.",
            ));
        }
        let stage = installed.with_extension("partial");
        let _ = fs::remove_dir_all(&stage);
        private_directory(&stage)?;
        let mut cleanup = Cleanup {
            path: stage.clone(),
            armed: true,
        };
        for asset in archives {
            let archive = stage.join("native-runtime.whl");
            self.download(
                &asset.url,
                &archive,
                DownloadPolicy {
                    size: asset.size,
                    checksum: Some(&asset.sha256),
                    too_large: "Runtime exceeds its declared size.",
                    invalid: "Runtime verification failed. Download again.",
                },
                cancelled,
                &mut |_| {},
            )
            .await?;
            let extraction_root = stage.clone();
            let extraction_archive = archive.clone();
            let members = asset.members.clone();
            let cancellation = cancelled.child_token();
            let extraction_cancel = ExtractionCancellation(cancellation.clone());
            let owner = ExtractionOwner {
                cleanup,
                _lock: self._lock.clone(),
            };
            let extraction = tokio::task::spawn_blocking(move || {
                let result = crate::onnx_runtime::extract(
                    &extraction_archive,
                    &extraction_root,
                    &members,
                    &cancellation,
                );
                (result, owner)
            });
            // A dropped future cancels extraction; its owner keeps staging and the lock until it stops.
            let (result, owner) = extraction
                .await
                .map_err(|_| ProviderError::message(IO_FAILURE))?;
            cleanup = owner.cleanup;
            drop(extraction_cancel);
            result?;
            fs::remove_file(&archive).map_err(|_| ProviderError::message(IO_FAILURE))?;
        }
        if cancelled.is_cancelled() {
            return Err(ProviderError::message("Download cancelled."));
        }
        let stamp = manifest.stamp(device)?;
        OpenOptions::new()
            .write(true)
            .create_new(true)
            .mode(0o600)
            .open(stage.join(".native-ready"))
            .and_then(|mut file| file.write_all(stamp.as_bytes()))
            .map_err(|_| ProviderError::message(IO_FAILURE))?;
        // Keep an existing managed runtime until the replacement has been fully verified.
        let previous = installed.with_extension("previous");
        let _ = fs::remove_dir_all(&previous);
        let replacing = installed
            .try_exists()
            .map_err(|_| ProviderError::message(IO_FAILURE))?;
        if replacing {
            fs::rename(&installed, &previous).map_err(|_| ProviderError::message(IO_FAILURE))?;
        }
        if fs::rename(&stage, &installed).is_err() {
            if replacing {
                let _ = fs::rename(&previous, &installed);
            }
            return Err(ProviderError::message(IO_FAILURE));
        }
        cleanup.armed = false;
        if replacing {
            let _ = fs::remove_dir_all(previous);
        }
        Ok(())
    }
    async fn download(
        &self,
        url: &str,
        path: &Path,
        policy: DownloadPolicy<'_>,
        cancelled: &CancellationToken,
        progress: &mut (dyn FnMut(u64) + Send),
    ) -> Result<()> {
        let failed = || {
            ProviderError::message(
                "The local artifact download failed. Check connectivity and retry.",
            )
        };
        let response = self
            .store
            .client
            .get(url)
            .send()
            .await
            .map_err(|_| failed())?;
        if !response.status().is_success() {
            return Err(ProviderError(format!(
                "HTTP Error {}: {}",
                response.status().as_u16(),
                response.status().canonical_reason().unwrap_or("")
            )));
        }
        let mut target = File::create(path).map_err(|_| ProviderError::message(IO_FAILURE))?;
        let mut digest = Sha256::new();
        let mut received = 0_u64;
        let mut stream = response.bytes_stream();
        let mut pending = Vec::with_capacity(BLOCK);
        while let Some(chunk) = stream.next().await {
            let chunk = chunk.map_err(|_| failed())?;
            let mut offset = 0;
            while offset < chunk.len() {
                let count = (BLOCK - pending.len()).min(chunk.len() - offset);
                pending.extend_from_slice(&chunk[offset..offset + count]);
                offset += count;
                if pending.len() == BLOCK {
                    write_block(
                        &pending,
                        &mut target,
                        &mut digest,
                        &mut received,
                        &policy,
                        cancelled,
                        progress,
                    )?;
                    pending.clear();
                }
            }
        }
        if !pending.is_empty() {
            write_block(
                &pending,
                &mut target,
                &mut digest,
                &mut received,
                &policy,
                cancelled,
                progress,
            )?;
        }
        let actual = digest
            .finalize()
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        if received != policy.size || policy.checksum.is_some_and(|expected| actual != expected) {
            return Err(ProviderError::message(policy.invalid));
        }
        Ok(())
    }
}
fn write_block(
    block: &[u8],
    target: &mut File,
    digest: &mut Sha256,
    received: &mut u64,
    policy: &DownloadPolicy<'_>,
    cancelled: &CancellationToken,
    progress: &mut (dyn FnMut(u64) + Send),
) -> Result<()> {
    if cancelled.is_cancelled() {
        return Err(ProviderError::message(CANCELLED));
    }
    *received += block.len() as u64;
    if *received > policy.size {
        return Err(ProviderError::message(policy.too_large));
    }
    digest.update(block);
    target
        .write_all(block)
        .map_err(|_| ProviderError::message(IO_FAILURE))?;
    progress(*received);
    Ok(())
}
fn part_path(path: &Path) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".part");
    name.into()
}

/// Apply the released data-filter containment, link and permission rules.
pub fn unpack_runtime(archive: &Path, destination: &Path, budget: u64) -> Result<()> {
    let invalid = || ProviderError::message(ARCHIVE_FAILURE);
    let io_failure = || ProviderError::message(IO_FAILURE);
    let open = || {
        File::open(archive)
            .map(flate2::read::MultiGzDecoder::new)
            .map(tar::Archive::new)
            .map_err(|_| invalid())
    };
    let mut source = open()?;
    let mut total = 0_u64;
    for member in source.entries().map_err(|_| invalid())? {
        let member = member.map_err(|_| invalid())?;
        total = total.saturating_add(member.size());
        if total > budget {
            return Err(ProviderError::message(
                "Runtime exceeds its storage budget.",
            ));
        }
    }
    let root = resolve_path(destination).map_err(|_| io_failure())?;
    let mut directories = vec![];
    let mut source = open()?;
    for member in source.entries().map_err(|_| invalid())? {
        let mut member = member.map_err(|_| invalid())?;
        let raw = member.path().map_err(|_| invalid())?.into_owned();
        let bytes = raw.as_os_str().as_bytes();
        let stripped = bytes
            .iter()
            .position(|byte| *byte != b'/')
            .unwrap_or(bytes.len());
        let name = Path::new(std::ffi::OsStr::from_bytes(&bytes[stripped..]));
        let target = destination.join(name);
        let resolved = resolve_path(&target).map_err(|_| io_failure())?;
        if !resolved.starts_with(&root) {
            return Err(invalid());
        }
        let kind = member.header().entry_type();
        let mut mode = member.header().mode().map_err(|_| invalid())? & 0o755;
        let time = member.header().mtime().map_err(|_| invalid())?;
        if kind.is_dir() {
            fs::create_dir_all(&target).map_err(|_| io_failure())?;
            directories.push((target, time));
            continue;
        }
        if kind.is_file() {
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|_| io_failure())?;
            }
            let mut output = File::create(&target).map_err(|_| io_failure())?;
            std::io::copy(&mut member, &mut output).map_err(|_| invalid())?;
        } else if kind.is_symlink() || kind.is_hard_link() {
            if resolved == root {
                return Err(invalid());
            }
            let link = member
                .link_name()
                .map_err(|_| invalid())?
                .ok_or_else(invalid)?
                .into_owned();
            if link.is_absolute() {
                return Err(invalid());
            }
            let link = normalize_link(&link);
            let base = if kind.is_symlink() {
                target.parent().unwrap_or(destination)
            } else {
                destination
            };
            let link_target = resolve_path(&base.join(&link)).map_err(|_| io_failure())?;
            if !link_target.starts_with(&root) {
                return Err(invalid());
            }
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent).map_err(|_| io_failure())?;
            }
            if target.symlink_metadata().is_ok() {
                fs::remove_file(&target).map_err(|_| io_failure())?;
            }
            if kind.is_symlink() {
                std::os::unix::fs::symlink(&link, &target).map_err(|_| io_failure())?;
                continue;
            }
            fs::hard_link(destination.join(&link), &target).map_err(|_| invalid())?;
        } else {
            return Err(invalid());
        }
        if mode & 0o100 == 0 {
            mode &= !0o111;
        }
        mode |= 0o600;
        fs::set_permissions(&target, fs::Permissions::from_mode(mode)).map_err(|_| io_failure())?;
        set_mtime(&target, time).map_err(|_| io_failure())?;
    }
    directories.sort_by(|a, b| a.0.cmp(&b.0));
    for (path, time) in directories.into_iter().rev() {
        set_mtime(&path, time).map_err(|_| io_failure())?;
    }
    Ok(())
}
fn normalize_link(path: &Path) -> PathBuf {
    use std::path::Component;
    let mut result = PathBuf::new();
    for component in path.components() {
        match component {
            Component::CurDir => {}
            Component::ParentDir if result.file_name().is_some_and(|name| name != "..") => {
                result.pop();
            }
            _ => result.push(component.as_os_str()),
        }
    }
    if result.as_os_str().is_empty() {
        result.push(".");
    }
    result
}
fn set_mtime(path: &Path, seconds: u64) -> std::io::Result<()> {
    let modified = std::time::UNIX_EPOCH
        .checked_add(Duration::from_secs(seconds))
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "Invalid archive timestamp",
            )
        })?;
    File::open(path)?.set_times(fs::FileTimes::new().set_modified(modified))
}
