//! Freeze capability overrides before starting the inherited app-server configuration.
use crate::{ProviderError, Result};
use mluva_core::{executables::find_executable, private_files::resolve_path};
use serde_json::Value;
use std::collections::BTreeMap;
use std::ffi::{OsStr, OsString};
use std::path::PathBuf;
use std::sync::LazyLock;

pub static TEXT_ONLY_CONFIG: LazyLock<Value> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/codex-text-only.json"))
        .expect("released capability overrides")
});

pub fn child_environment(
    environ: impl IntoIterator<Item = (OsString, OsString)>,
) -> BTreeMap<OsString, OsString> {
    const ALLOWED: &[&str] = &[
        "PATH",
        "HOME",
        "USER",
        "LOGNAME",
        "LANG",
        "LC_ALL",
        "LC_CTYPE",
        "CODEX_HOME",
        "OPENAI_API_KEY",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "XDG_CACHE_HOME",
        "XDG_STATE_HOME",
        "XDG_RUNTIME_DIR",
        "DBUS_SESSION_BUS_ADDRESS",
        "HTTPS_PROXY",
        "HTTP_PROXY",
        "ALL_PROXY",
        "NO_PROXY",
        "https_proxy",
        "http_proxy",
        "all_proxy",
        "no_proxy",
        "SSL_CERT_FILE",
        "SSL_CERT_DIR",
    ];
    environ
        .into_iter()
        .filter(|(key, _)| ALLOWED.iter().any(|allowed| key == OsStr::new(allowed)))
        .collect()
}

pub fn isolated_command(
    mut command: Vec<OsString>,
    environment: &BTreeMap<OsString, OsString>,
) -> Result<Vec<OsString>> {
    let home = environment
        .get(OsStr::new("CODEX_HOME"))
        .map(PathBuf::from)
        .or_else(|| {
            environment
                .get(OsStr::new("HOME"))
                .map(|home| PathBuf::from(home).join(".codex"))
        })
        .ok_or_else(|| ProviderError::message("Codex app-server could not start."))?;
    let home = resolve_path(&home)
        .map_err(|_| ProviderError::message("Codex app-server could not start."))?;
    let existing = ["AGENTS.md", "AGENTS.override.md"]
        .into_iter()
        .map(|name| home.join(name))
        .filter(|path| path.is_file())
        .collect::<Vec<_>>();
    if existing.is_empty() {
        return Ok(command);
    }
    let bwrap = find_executable("bwrap").ok_or_else(|| {
        ProviderError::message(
            "Install bubblewrap to isolate Codex's global instructions before rewriting.",
        )
    })?;
    let mut wrapped = vec![
        bwrap.into_os_string(),
        "--die-with-parent".into(),
        "--new-session".into(),
        "--bind".into(),
        "/".into(),
        "/".into(),
    ];
    for path in existing {
        wrapped.extend([
            "--ro-bind".into(),
            "/dev/null".into(),
            path.into_os_string(),
        ]);
    }
    wrapped.push("--".into());
    wrapped.append(&mut command);
    Ok(wrapped)
}
