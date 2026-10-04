//! Actual async scheduler and Codex children against unchanged released behavior.
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

fn peer() -> PathBuf {
    let target = std::env::var_os("CARGO_TARGET_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target"));
    let peer = target.join("debug/codex-fixture-peer");
    assert!(
        peer.is_file(),
        "Build native provider fixture binaries first"
    );
    peer.canonicalize().unwrap()
}
#[test]
fn ordered_bounded_cleanup_and_owned_children_match_release() {
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-segment-cleanup.json")).unwrap();
    assert_eq!(
        fixture["reference_commit"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    for row in fixture["cases"].as_array().unwrap() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path();
        for name in ["evidence", "home", "codex", "tmp"] {
            fs::create_dir(root.join(name)).unwrap();
        }
        fs::write(root.join("case.json"), serde_json::to_vec(row).unwrap()).unwrap();
        let mut controls = row["controls"].clone();
        for (index, control) in controls.as_object_mut().unwrap().values_mut().enumerate() {
            if control["hold"] == true {
                control["gate"] = json!(root.join(format!("controlled-{index}.release")));
            }
            control.as_object_mut().unwrap().remove("hold");
        }
        let specification = json!({"scenario":row["scenario"].as_str().unwrap_or("clean"),"evidence":root.join("evidence"),"segment_controls":controls});
        fs::write(
            root.join("codex/fixture.json"),
            serde_json::to_vec(&specification).unwrap(),
        )
        .unwrap();
        let output = Command::new(env!("CARGO_BIN_EXE_segment-cleanup-fixture-peer"))
            .args([root.as_os_str(), peer().as_os_str()])
            .env_clear()
            .env("PATH", "/usr/bin:/bin")
            .env("HOME", root.join("home"))
            .env("CODEX_HOME", root.join("codex"))
            .env("TMPDIR", root.join("tmp"))
            .env("LANG", "C.UTF-8")
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}: native driver failed: {}",
            row["name"],
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Value = serde_json::from_slice(&output.stdout).unwrap();
        let mut expected = json!({});
        for key in [
            "accepted",
            "observations",
            "terminal",
            "finished",
            "after_stop",
            "second",
            "closed",
            "prompts",
            "factory_calls",
            "processes",
        ] {
            expected[key] = row[key].clone();
        }
        if actual != expected {
            fs::write(
                Path::new(env!("CARGO_MANIFEST_DIR"))
                    .join("../../tmp/segment-cleanup-mismatch.json"),
                serde_json::to_vec_pretty(
                    &json!({"case":row["name"],"actual":actual,"expected":expected}),
                )
                .unwrap(),
            )
            .unwrap();
            panic!(
                "Released segment cleanup differs: {}; report in tmp/segment-cleanup-mismatch.json",
                row["name"]
            );
        }
    }
}
