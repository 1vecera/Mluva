//! Synthetic filesystem/public asset-installer driver; never installed.
use mluva_providers::{
    ProviderError,
    local_assets::{AssetStore, ModelSpec, QwenManifest, disk_usage, unpack_runtime},
};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::io::Read;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use tokio_util::sync::CancellationToken;

fn observed(value: Result<(), ProviderError>) -> Value {
    match value {
        Ok(()) => json!({"ok":null}),
        Err(error)
            if error.to_string()
                == "The runtime archive is invalid or contains an unsafe path." =>
        {
            json!({"error_kind":"archive_error"})
        }
        Err(error) if error.to_string() == "Local model files could not be read or written." => {
            json!({"error_kind":"filesystem_error"})
        }
        Err(error)
            if error.to_string()
                == "The local artifact download failed. Check connectivity and retry." =>
        {
            json!({"error_kind":"download_error"})
        }
        Err(error) => json!({"error":error.to_string()}),
    }
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn snapshot(root: &Path, mtime_paths: &[Value]) -> Value {
    fn walk(root: &Path, path: &Path, rows: &mut Vec<Value>, mtime_paths: &[Value]) {
        let mut entries = std::fs::read_dir(path)
            .unwrap()
            .map(Result::unwrap)
            .collect::<Vec<_>>();
        entries.sort_by_key(|entry| entry.file_name());
        for entry in entries {
            let path = entry.path();
            let metadata = path.symlink_metadata().unwrap();
            let name = path.strip_prefix(root).unwrap().to_string_lossy();
            let mut row = if metadata.is_symlink() {
                json!({"path":name,"kind":"link","target":std::fs::read_link(&path).unwrap()})
            } else if metadata.is_dir() {
                json!({"path":name,"kind":"directory","mode":metadata.permissions().mode()&0o777})
            } else {
                let hash = if metadata.len() < 8_000_000 {
                    Some(hex(&Sha256::digest(std::fs::read(&path).unwrap())))
                } else {
                    None
                };
                json!({"path":name,"kind":"file","size":metadata.len(),"mode":metadata.permissions().mode()&0o777,"sha256":hash})
            };
            if !metadata.is_symlink()
                && mtime_paths
                    .iter()
                    .any(|path| path.as_str() == Some(name.as_ref()))
            {
                row["mtime"] = json!(
                    metadata
                        .modified()
                        .unwrap()
                        .duration_since(std::time::UNIX_EPOCH)
                        .unwrap()
                        .as_secs()
                );
            }
            rows.push(row);
            if metadata.is_dir() {
                walk(root, &path, rows, mtime_paths);
            }
        }
    }
    let mut rows = vec![];
    walk(root, root, &mut rows, mtime_paths);
    json!(rows)
}

#[tokio::main(flavor = "current_thread")]
async fn main() {
    let mut input = vec![];
    std::io::stdin().read_to_end(&mut input).unwrap();
    let spec: Value = serde_json::from_slice(&input).unwrap();
    let root = std::env::var_os("LOCAL_ASSET_FIXTURE_ROOT").unwrap();
    let root = Path::new(&root);
    let data = root.join("xdg-data/mluva");
    let mut results = vec![];
    let mut ready = vec![];
    let mut progress = vec![];
    match spec["kind"].as_str().unwrap() {
        "model" => {
            let model: ModelSpec = serde_json::from_value(spec["model"].clone()).unwrap();
            let store = AssetStore::new(&data).unwrap();
            match store.begin_download() {
                Err(error) => {
                    for _ in spec["calls"].as_array().unwrap() {
                        results.push(observed(Err(error.clone())));
                        ready.push(model.ready(&data));
                        progress.push(vec![]);
                    }
                }
                Ok(mut download) => {
                    for call in spec["calls"].as_array().unwrap() {
                        let cancellation = CancellationToken::new();
                        if call["cancel"] == true {
                            cancellation.cancel();
                        }
                        let mut samples = vec![];
                        let mut sample = |value| {
                            samples.push(value);
                            if call["cancel_at_progress"]
                                .as_u64()
                                .is_some_and(|count| samples.len() as u64 >= count)
                            {
                                cancellation.cancel();
                            }
                        };
                        results.push(observed(
                            download
                                .install_model_files(&model, &cancellation, &mut sample)
                                .await,
                        ));
                        progress.push(samples);
                        ready.push(model.ready(&data));
                    }
                }
            }
        }
        "runtime" => {
            let manifest: QwenManifest = serde_json::from_value(spec["manifest"].clone()).unwrap();
            let device = spec["device"].as_str().unwrap();
            let store = AssetStore::new(&data).unwrap();
            match store.begin_download() {
                Err(error) => results.push(observed(Err(error))),
                Ok(mut download) => {
                    for call in spec["calls"].as_array().unwrap() {
                        let cancellation = CancellationToken::new();
                        if call["cancel"] == true {
                            cancellation.cancel();
                        }
                        results.push(observed(
                            download
                                .install_qwen_runtime(&manifest, device, &cancellation)
                                .await,
                        ));
                        ready.push(manifest.ready(&data, device));
                    }
                }
            }
        }
        "extract" => {
            let destination = data.join("unpacked");
            std::fs::create_dir_all(&destination).unwrap();
            results.push(observed(unpack_runtime(
                &root.join("archive.tar.gz"),
                &destination,
                spec["budget"].as_u64().unwrap(),
            )));
        }
        "readiness" => {
            if !spec["model"].is_null() {
                let model: ModelSpec = serde_json::from_value(spec["model"].clone()).unwrap();
                ready.push(model.ready(&data));
            } else {
                let manifest: QwenManifest =
                    serde_json::from_value(spec["manifest"].clone()).unwrap();
                ready.push(manifest.ready(&data, spec["device"].as_str().unwrap()));
            }
        }
        _ => panic!("unknown fixture kind"),
    }
    let paths = spec["mtime_paths"]
        .as_array()
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    println!(
        "{}",
        json!({"results":results,"ready":ready,"progress":progress,"snapshot":snapshot(&data,paths),"disk_usage":disk_usage(&data)})
    );
}
