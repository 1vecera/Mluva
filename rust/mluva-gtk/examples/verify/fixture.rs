//! Seed preferences and external PCM/HTTP peers; all application work stays in mluva.
use super::*;
use mluva_core::config::{AppConfig, AudioRetentionPolicy};
use std::os::unix::fs::{PermissionsExt, symlink};

#[path = "../../../mluva-workflows/tests/support/http.rs"]
mod http;

pub const RAW: &str = "Synthetic narration keeps 12 files.";
pub const EDIT: &str = "Synthetic working edit: třináct souborů 🦊.";

pub fn prepare(root: &Path, checkout: &Path, binaries: &Path) -> Result<http::Peer> {
    let bundle = root.join("bundle");
    fs::create_dir_all(bundle.join("bin"))?;
    // Copy the executable so current_exe resolves to this development bundle.
    for name in ["mluva", "mluva-audio-cleanup"] {
        fs::copy(binaries.join(name), bundle.join("bin").join(name))?;
    }
    symlink(
        checkout.join("rust/mluva-gtk/resources"),
        bundle.join("resources"),
    )?;
    let tools = root.join("tools");
    fs::create_dir(&tools)?;
    for (name, contents) in [
        ("secret-tool", "#!/bin/sh\nexit 1\n"),
        ("pw-dump", "#!/bin/sh\nprintf '[]\\n'\n"),
    ] {
        fs::write(tools.join(name), contents)?;
        fs::set_permissions(tools.join(name), fs::Permissions::from_mode(0o700))?;
    }
    symlink(binaries.join("audio-fixture-peer"), tools.join("pw-record"))?;
    fs::write(
        tools.join("test-config.json"),
        serde_json::to_vec(
            &json!({"pcm_hex":"01000200".repeat(8000),"fragments":[8192],"wait":true}),
        )?,
    )?;
    let peer = http::Peer::new(&[
        json!({"route":"speech","status":200,"payload":{"text":RAW,"language_code":"eng"}}),
    ]);
    AppConfig {
        transcription_provider: "litellm".into(),
        transcription_base_url: format!("{}/v1", peer.address),
        transcription_api_key_env: "MLUVA_UNUSED_SYNTHETIC_KEY".into(),
        welcome_completed: true,
        transcription_chunk_seconds: 30,
        audio_retention_policy: AudioRetentionPolicy::Never,
        ..Default::default()
    }
    .save(&path_env("XDG_CONFIG_HOME")?.join("mluva/config.json"))?;
    fs::write(
        root.join("fixture.json"),
        serde_json::to_vec_pretty(&json!({
            "application":bundle.join("bin/mluva"),"speech_endpoint":peer.address,
        "substitutions":["pw-record emits deterministic 16 kHz s16le mono PCM","loopback HTTP speech response","welcome completed; retention never; manual recording is copy-only","global shortcut setup disabled; secret-tool unavailable; pw-dump empty"],
            "smoke_checks":["production executable startup","production D-Bus actions","capture lifecycle and HTTP multipart upload","immutable History and editor autosave","external AT-SPI edit","normal Quit and reopening saved working text"],
            "not_proven":["live microphone","live providers","physical keys","sign-in","Wayland permissions","installed layout"]
        }))?,
    )?;
    Ok(peer)
}

pub fn check_requests(peer: &http::Peer, root: &Path) -> Result<()> {
    let requests = peer
        .observed
        .lock()
        .map_err(|_| "HTTP peer lock poisoned")?
        .clone();
    fs::write(
        root.join("provider-requests.json"),
        serde_json::to_vec_pretty(&requests)?,
    )?;
    require(
        requests.len() == 1 && requests[0]["path"] == "/v1/audio/transcriptions",
        "expected exactly one synthetic speech request",
    )?;
    use base64::{Engine as _, engine::general_purpose::STANDARD};
    let audio = STANDARD.decode(
        requests[0]["fields"]["file"]
            .as_str()
            .ok_or("speech request lacks uploaded audio")?,
    )?;
    require(
        audio.starts_with(b"RIFF") && audio.len() > 44,
        "provider did not receive captured WAV audio",
    )?;
    Ok(())
}
