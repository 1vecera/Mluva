use serde_json::Value;
use std::fs;
use std::os::unix::fs::{PermissionsExt, symlink};
use std::path::{Path, PathBuf};
use std::thread;
use std::time::{Duration, Instant};

pub struct Peer {
    pub directory: tempfile::TempDir,
    pub executable: PathBuf,
}

impl Peer {
    pub fn new(config: &Value) -> Self {
        let directory = tempfile::tempdir().unwrap();
        let config_path = directory.path().join("test-config.json");
        fs::write(&config_path, serde_json::to_vec(config).unwrap()).unwrap();
        fs::set_permissions(config_path, fs::Permissions::from_mode(0o600)).unwrap();
        let executable = directory.path().join("pw-record");
        symlink(env!("CARGO_BIN_EXE_audio-fixture-peer"), &executable).unwrap();
        Self {
            directory,
            executable,
        }
    }

    pub fn path(&self) -> &Path {
        self.directory.path()
    }

    pub fn ready(&self, name: &str) -> Value {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Ok(contents) = fs::read(self.path().join(format!("{name}.ready.json")))
                && let Ok(value) = serde_json::from_slice(&contents)
            {
                return value;
            }
            assert!(
                Instant::now() < deadline,
                "Synthetic {name} subprocess never became ready"
            );
            thread::sleep(Duration::from_millis(5));
        }
    }
}

pub fn reference() -> Value {
    serde_json::from_str(include_str!("../fixtures/released-audio.json")).unwrap()
}

pub fn unhex(value: &str) -> Vec<u8> {
    (0..value.len())
        .step_by(2)
        .map(|offset| u8::from_str_radix(&value[offset..offset + 2], 16).unwrap())
        .collect()
}

pub fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}

pub fn mode(path: &Path) -> u32 {
    path.metadata().unwrap().permissions().mode() & 0o777
}

pub fn assert_level(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 1e-14,
        "RMS {actual} != released {expected}"
    );
}
