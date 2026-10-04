//! Preserve the installed launcher's opt-in credential routes before application startup.
//! No credential catalog is read here; selected launchers own secret resolution.
use serde_json::Value;
use std::{
    env,
    ffi::{CString, OsString},
    fs,
    os::unix::{ffi::OsStrExt, process::CommandExt},
    path::{Path, PathBuf},
    process::Command,
};

pub const START_ERROR: &str = "Mluva could not start its managed credential profile. Check the configured launcher and try again.";

fn nonempty(name: &str) -> Option<OsString> {
    env::var_os(name).filter(|value| !value.is_empty())
}
fn executable(path: &Path) -> bool {
    CString::new(path.as_os_str().as_bytes()).is_ok_and(|path| unsafe {
        libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0
    })
}
fn populated(path: &Path) -> bool {
    fs::metadata(path).is_ok_and(|metadata| metadata.len() > 0)
}
fn probe_json(bytes: &[u8]) -> Option<Value> {
    std::str::from_utf8(bytes).ok()?;
    // The released JSON reader admits non-finite constants and escaped lone
    // surrogates. Neither can equal our ASCII provider/key, but rejecting an
    // unrelated field would incorrectly request credentials for a local model.
    // Normalize only this in-memory probe; never rewrite the settings file.
    let mut normalized = Vec::with_capacity(bytes.len());
    let mut quoted = false;
    let mut index = 0;
    while index < bytes.len() {
        let byte = bytes[index];
        if quoted && byte == b'\\' {
            if bytes.get(index + 1) == Some(&b'u')
                && let Some(hex) = bytes.get(index + 2..index + 6)
                && let Ok(hex) = std::str::from_utf8(hex)
                && let Ok(value) = u16::from_str_radix(hex, 16)
                && (0xd800..=0xdfff).contains(&value)
            {
                normalized.extend_from_slice(br"\ufffd");
                index += 6;
                continue;
            }
            let end = (index + 2).min(bytes.len());
            normalized.extend_from_slice(&bytes[index..end]);
            index = end;
            continue;
        }
        if !quoted
            && let Some(token) = [b"-Infinity".as_slice(), b"Infinity", b"NaN"]
                .iter()
                .find(|token| bytes[index..].starts_with(token))
        {
            normalized.extend_from_slice(b"null");
            index += token.len();
            continue;
        }
        if byte == b'"' {
            quoted = !quoted;
        }
        normalized.push(byte);
        index += 1;
    }
    serde_json::from_slice(&normalized).ok()
}
fn needs_credential(config: &Path) -> bool {
    if !config.is_file() {
        return true;
    }
    let Some(Value::Object(values)) = fs::read(config).ok().and_then(|bytes| probe_json(&bytes))
    else {
        return true;
    };
    values
        .get("transcription_provider")
        .is_none_or(|provider| provider == "elevenlabs")
}

/// Replace this process through the first eligible managed route, at most once.
/// With no applicable route, continue in this process with the existing environment.
pub fn inherit_managed_profile(arguments: impl IntoIterator<Item = OsString>) -> Result<(), u8> {
    // The released wrapper probes the provider before considering an inherited
    // profile/key. Keep malformed or missing configurations on its default route.
    let config_home = nonempty("XDG_CONFIG_HOME")
        .map(PathBuf::from)
        .or_else(|| {
            env::var_os("HOME").map(|mut home| {
                home.push("/.config");
                PathBuf::from(home)
            })
        })
        .ok_or(1_u8)?;
    if !needs_credential(&config_home.join("mluva/config.json"))
        || env::var_os("MLUVA_SECRET_PROFILE").as_deref() == Some(std::ffi::OsStr::new("1"))
        || [
            "ELEVENLABS_API_KEY",
            "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
            "ELEVEN_LABS_STT_TOKEN",
        ]
        .iter()
        .any(|name| nonempty(name).is_some())
    {
        return Ok(());
    }
    let directory = nonempty("DAS_CONF_DIR")
        .map(PathBuf::from)
        .unwrap_or_else(|| config_home.join("daniel-ai-skills"));
    let snapshot = directory.join("bin/das-agent-snapshot");
    let scoped = directory.join("bin/das-mcp-launch");
    let agent = directory.join("bin/das-agent-launch");
    let mut command = if directory.join("snapshot.enabled").is_file() && executable(&snapshot) {
        let mut command = Command::new(snapshot);
        command
            .args(["launch", "--only"])
            .arg(
                nonempty("MLUVA_AGENT_SECRET_NAME")
                    .unwrap_or_else(|| "ELEVEN_LABS_STT_TOKEN".into()),
            )
            .arg("--");
        command
    } else if executable(&scoped) && populated(&directory.join("env/mluva.env")) {
        let mut command = Command::new(scoped);
        command.args(["mluva", "--"]);
        command
    } else if executable(&agent) && populated(&directory.join("env/agent.env")) {
        let mut command = Command::new(agent);
        command
            .arg("--only")
            .arg(
                nonempty("MLUVA_AGENT_SECRET_NAME")
                    .unwrap_or_else(|| "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL".into()),
            )
            .arg("--");
        command
    } else {
        return Ok(());
    };
    command
        .args(["env", "MLUVA_SECRET_PROFILE=1"])
        .args(arguments);
    // exec retains the PID, standard streams, signals and selected helper's exit
    // status. Never fall through to another profile after the chosen route fails.
    let _ = command.exec();
    Err(126)
}
