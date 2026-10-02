//! Sparse readiness and external peers for local protocol and capture checks.

use mluva_providers::{
    local_asr::OnnxOptions,
    local_assets::{MODEL_CATALOG, QWEN_RUNTIME},
    onnx_runtime::ONNX_RUNTIME,
};
use serde_json::{Value, json};
use std::{fs, os::unix::fs::PermissionsExt, path::Path};

fn sparse(path: &Path, size: u64) {
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::File::create(path).unwrap().set_len(size).unwrap();
}

pub fn setup(root: &Path, spec: &Value, qwen_peer: &Path, onnx_peer: &Path) -> OnnxOptions {
    let data = root.join("xdg-data/mluva");
    fs::create_dir_all(&data).unwrap();
    fs::set_permissions(&data, fs::Permissions::from_mode(0o700)).unwrap();
    let model = MODEL_CATALOG
        .iter()
        .find(|model| model.id == spec["model"].as_str().unwrap())
        .unwrap();
    let directory = model.path(&data);
    for file in &model.files {
        sparse(&directory.join(&file.name), file.size);
    }
    fs::write(directory.join(".ready"), &model.revision).unwrap();
    let device = spec["device"].as_str().unwrap();
    let mut config = spec.clone();
    config["root"] = json!(root);
    if model.id == "qwen3-1.7b" {
        let binary = QWEN_RUNTIME.binary(&data, device);
        fs::create_dir_all(binary.parent().unwrap()).unwrap();
        fs::copy(qwen_peer, &binary).unwrap();
        fs::set_permissions(&binary, fs::Permissions::from_mode(0o755)).unwrap();
        fs::write(
            data.join("qwen-runtime").join(device).join(".ready"),
            &QWEN_RUNTIME.assets[device].sha256,
        )
        .unwrap();
        fs::write(
            binary.with_file_name("fixture.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
    } else {
        fs::copy(onnx_peer, root.join("worker")).unwrap();
        fs::set_permissions(root.join("worker"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(
            root.join("fixture.json"),
            serde_json::to_vec(&config).unwrap(),
        )
        .unwrap();
        if device == "cuda" {
            let runtime = ONNX_RUNTIME.root(&data, "cuda");
            fs::create_dir_all(&runtime).unwrap();
            for member in ONNX_RUNTIME
                .cuda
                .iter()
                .flat_map(|archive| &archive.members)
            {
                sparse(&runtime.join(&member.path), member.size);
            }
            fs::write(
                runtime.join(".native-ready"),
                ONNX_RUNTIME.stamp("cuda").unwrap(),
            )
            .unwrap();
        }
    }
    let mut options = OnnxOptions::new(data, model.id.as_str(), root.join("worker"));
    options.device = device.into();
    options
}

pub fn trace(root: &Path) -> Vec<Value> {
    let raw = fs::read_to_string(root.join("trace.jsonl")).unwrap_or_default();
    let Some(end) = raw.rfind('\n') else {
        return vec![];
    };
    raw[..end]
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect()
}
