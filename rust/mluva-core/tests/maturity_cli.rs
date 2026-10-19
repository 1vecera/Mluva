//! Observe the real documentation command, with only its output directory mounted privately.
use std::{fs, path::Path, process::Command};

#[test]
fn generated_matrix_describes_current_features_and_checks_never_overwrite_edits() {
    let root = tempfile::Builder::new()
        .prefix("maturity-cli-")
        .tempdir()
        .unwrap();
    let directory = root.path().join("docs");
    fs::create_dir(&directory).unwrap();
    let output = directory.join("feature-maturity.md");
    let repository = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap();
    let run = |argument: &str| {
        Command::new("/usr/bin/bwrap")
            .args([
                "--die-with-parent",
                "--ro-bind",
                "/",
                "/",
                "--dev",
                "/dev",
                "--bind",
            ])
            .arg(&directory)
            .arg(repository.join("docs"))
            .arg("--")
            .arg(env!("CARGO_BIN_EXE_mluva-feature-maturity"))
            .arg(argument)
            .env_clear()
            .output()
            .unwrap()
    };
    let stale = b"docs/feature-maturity.md is stale; run `make linux-feature-maturity`.\n";
    let missing = run("--check");
    assert_eq!(missing.status.code(), Some(1));
    assert_eq!(missing.stderr, stale);
    assert!(!output.exists());
    let written = run("--write");
    assert!(
        written.status.success(),
        "{}",
        String::from_utf8_lossy(&written.stderr)
    );
    assert!(written.stdout.is_empty() && written.stderr.is_empty());
    // Preserve the released registry while adding the explicitly experimental native feature.
    let generated = fs::read_to_string(&output).unwrap();
    let added = "| Hold to talk to Herdr agents |";
    assert!(
        generated
            .split("## Experimental")
            .nth(1)
            .unwrap()
            .contains(added)
    );
    assert_eq!(
        generated
            .lines()
            .filter(|line| line.starts_with(added))
            .count(),
        1
    );
    assert!(generated.contains("| Speech provider choices |"));
    assert!(generated.contains("| Conversations |"));
    for retired in [
        "| Live structured rewriting |",
        "| Command mode |",
        "| Notes mode |",
        "| Custom saved styles |",
        "| Faithful cleanup |",
    ] {
        assert!(!generated.contains(retired));
    }
    let checked = run("--check");
    assert!(checked.status.success());
    assert!(checked.stdout.is_empty() && checked.stderr.is_empty());
    fs::write(&output, "uncommitted documentation\n").unwrap();
    let changed = run("--check");
    assert_eq!(changed.status.code(), Some(1));
    assert_eq!(changed.stderr, stale);
    assert_eq!(fs::read(&output).unwrap(), b"uncommitted documentation\n");
}
