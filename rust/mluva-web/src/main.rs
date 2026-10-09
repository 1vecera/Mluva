//! Opt-in companion; binds loopback and never opens a microphone or desktop window.

use mluva_core::{
    config::{AppConfig, AppPaths},
    executables::find_executable,
    history::HistoryStore,
};
use mluva_providers::{
    credentials::CredentialStore,
    elevenlabs::{ElevenLabsClient, SCRIBE_ENDPOINT},
};
use mluva_web::{AppState, auth::Access, router};
use std::{collections::BTreeMap, sync::Arc, time::Duration};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    if std::env::args().any(|arg| arg == "--help") {
        println!(
            "mluva-web: private browser recorder on 127.0.0.1:8787\nRequires MLUVA_WEB_ORIGIN, MLUVA_WEB_ACCESS_TEAM and MLUVA_WEB_ACCESS_AUDIENCE.\nUses Mluva settings/history, ElevenLabs Scribe, ffmpeg and wl-copy. See docs/browser-recorder.md."
        );
        return Ok(());
    }
    let environ: BTreeMap<_, _> = std::env::vars().collect();
    let paths = AppPaths::from_environ(&environ)?;
    let config = AppConfig::load(&paths.config.join("config.json"))?;
    let origin = std::env::var("MLUVA_WEB_ORIGIN")
        .map_err(|_| "Set MLUVA_WEB_ORIGIN to your HTTPS recorder URL.")?;
    let url = reqwest::Url::parse(&origin)?;
    if url.scheme() != "https" || url.origin().ascii_serialization() != origin {
        return Err("MLUVA_WEB_ORIGIN must be an HTTPS origin with no path.".into());
    }
    let access = Arc::new(
        Access::new(
            std::env::var("MLUVA_WEB_ACCESS_TEAM")?,
            std::env::var("MLUVA_WEB_ACCESS_AUDIENCE")?,
        )
        .await?,
    );
    let key = CredentialStore::new().elevenlabs_api_key().await?;
    let speech = Arc::new(ElevenLabsClient::new(
        key,
        SCRIBE_ENDPOINT,
        Duration::from_secs(1800),
    )?);
    let ffmpeg = find_executable("ffmpeg").ok_or("Install ffmpeg to receive browser audio.")?;
    let clipboard =
        find_executable("wl-copy").ok_or("Install wl-clipboard for PC clipboard delivery.")?;
    if !config.incognito_mode {
        HistoryStore::new(paths.data.join("history.sqlite3")).initialize()?;
    }
    let state = Arc::new(AppState::new(
        access, origin, config, paths, speech, clipboard, ffmpeg,
    ));
    let port: u16 = std::env::var("MLUVA_WEB_PORT")
        .unwrap_or_else(|_| "8787".into())
        .parse()?;
    let listener = tokio::net::TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!(
        "Mluva browser recorder listening on 127.0.0.1:{port}. Cloudflare Access is required."
    );
    axum::serve(listener, router(state))
        .with_graceful_shutdown(async {
            let _ = tokio::signal::ctrl_c().await;
        })
        .await?;
    Ok(())
}
