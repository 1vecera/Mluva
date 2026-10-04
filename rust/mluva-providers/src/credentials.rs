//! Optional desktop keyring storage and the released speech-credential precedence.
use crate::{ProviderError, Result, Secret};
use mluva_core::{executables::find_executable, text};
use std::collections::HashMap;
use std::process::{Output, Stdio};
use std::time::Duration;
use tokio::io::AsyncWriteExt;
use tokio::process::Command;
use tokio::sync::Mutex;

pub const SPEECH_KEY_VARIABLES: &[&str] = &[
    "ELEVENLABS_API_KEY",
    "DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL",
    "ELEVEN_LABS_STT_TOKEN",
];
const UNAVAILABLE: &str =
    "Desktop keyring unavailable. Set ELEVENLABS_API_KEY in your launch environment instead.";
const INVALID_TEXT: &str = "Desktop keyring returned invalid text.";
const MISSING: &str = "ElevenLabs credential unavailable. Set ELEVENLABS_API_KEY in Mluva's process environment through a secret manager or session service.";

/// Validate the exact entered text; Unicode whitespace and character counts
/// retain the released behavior, and successful storage never trims the key.
pub fn validate_speech_key(key: &str) -> Result<()> {
    if key.is_empty() || key.chars().count() > 4096 || key.chars().any(text::whitespace) {
        return Err(ProviderError::message("Enter a valid API key."));
    }
    Ok(())
}

/// Explicit environment maps do not trigger a desktop keyring lookup.
pub fn speech_key_from_environment(
    environ: &HashMap<String, String>,
    variable_names: &[&str],
) -> Result<Secret> {
    for name in variable_names {
        if let Some(value) = environ.get(*name)
            && !value.is_empty()
            && !value.chars().all(text::whitespace)
        {
            return Ok(Secret::new(value));
        }
    }
    Err(ProviderError::message(MISSING))
}

/// One application-owned cache, including an unavailable/empty lookup. Store
/// completion invalidates it; failed saves retain the previous cached value.
#[derive(Default)]
pub struct CredentialStore {
    cached: Mutex<Option<Option<Secret>>>,
}
impl CredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stored_speech_key(&self) -> Result<Option<Secret>> {
        let mut cache = self.cached.lock().await;
        if let Some(value) = cache.as_ref() {
            return Ok(value.clone());
        }
        let output = run_tool(
            &["lookup", "application", "mluva", "provider", "elevenlabs"],
            None,
            Duration::from_secs(2),
        )
        .await;
        let value = match output {
            Some(output) => {
                let stdout = decoded_output(output)?;
                if stdout.is_empty() {
                    None
                } else {
                    Some(Secret::new(stdout))
                }
            }
            None => None,
        };
        *cache = Some(value.clone());
        Ok(value)
    }

    pub async fn elevenlabs_api_key(&self) -> Result<Secret> {
        if let Some(key) = self.stored_speech_key().await? {
            return Ok(key);
        }
        for name in SPEECH_KEY_VARIABLES {
            if let Some(value) = std::env::var_os(name) {
                let value = value.into_string().map_err(|_| {
                    ProviderError::message("ElevenLabs credential contains invalid text.")
                })?;
                if !value.is_empty() && !value.chars().all(text::whitespace) {
                    return Ok(Secret::new(value));
                }
            }
        }
        Err(ProviderError::message(MISSING))
    }

    pub async fn store_speech_key(&self, key: &str) -> Result<()> {
        validate_speech_key(key)?;
        let output = run_tool(
            &[
                "store",
                "--label=Mluva ElevenLabs API key",
                "application",
                "mluva",
                "provider",
                "elevenlabs",
            ],
            Some(key.as_bytes()),
            Duration::from_secs(60),
        )
        .await
        .ok_or_else(|| ProviderError::message(UNAVAILABLE))?;
        // The text-mode reference decodes both captured streams, even when
        // their contents are not used. Never include those bytes in an error.
        std::str::from_utf8(&output.stdout)
            .and_then(|_| std::str::from_utf8(&output.stderr))
            .map_err(|_| ProviderError::message(INVALID_TEXT))?;
        if !output.status.success() {
            return Err(ProviderError::message(UNAVAILABLE));
        }
        *self.cached.lock().await = None;
        Ok(())
    }
}

fn decoded_output(output: Output) -> Result<String> {
    let stdout = std::str::from_utf8(&output.stdout)
        .and_then(|stdout| std::str::from_utf8(&output.stderr).map(|_| stdout))
        .map_err(|_| ProviderError::message(INVALID_TEXT))?;
    if !output.status.success() {
        return Ok(String::new());
    }
    let stdout = stdout.replace("\r\n", "\n").replace('\r', "\n");
    Ok(text::trim(&stdout).into())
}

async fn run_tool(args: &[&str], input: Option<&[u8]>, timeout: Duration) -> Option<Output> {
    let executable = find_executable("secret-tool")?;
    let mut command = Command::new(executable);
    command
        .args(args)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    if input.is_some() {
        command.stdin(Stdio::piped());
    }
    let child = command.spawn().ok()?;
    tokio::time::timeout(timeout, async {
        let mut child = child;
        let stdin = child.stdin.take();
        let write = async {
            if let Some(input) = input {
                let mut stdin = stdin?;
                match stdin.write_all(input).await {
                    Err(error)
                        if error.kind() != std::io::ErrorKind::BrokenPipe
                            && error.raw_os_error() != Some(libc::EINVAL) =>
                    {
                        return None;
                    }
                    _ => {}
                }
                drop(stdin);
            }
            Some(())
        };
        let (written, output) = tokio::join!(write, child.wait_with_output());
        written?;
        output.ok()
    })
    .await
    .ok()
    .flatten()
}
