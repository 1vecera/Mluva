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
/// Azure Speech key aliases, in precedence order; the region has its own variable.
pub const MICROSOFT_KEY_VARIABLES: &[&str] = &["AZURE_SPEECH_KEY", "SPEECH_STUDIO_KEY"];
pub const MICROSOFT_REGION_VARIABLE: &str = "AZURE_SPEECH_REGION";
pub const XAI_KEY_VARIABLES: &[&str] = &["XAI_API_KEY", "DAS_ITEM_GROK_STT_API_KEY__KEY"];
pub const MISTRAL_KEY_VARIABLES: &[&str] = &["MISTRAL_API_KEY", "DAS_ITEM_MISTRAL_API_KEY__KEY"];
const INVALID_TEXT: &str = "Desktop keyring returned invalid text.";

/// A cloud speech service with an API key. Each has its own desktop-keyring entry
/// (`application mluva provider <id>`) and environment aliases; keys are never shared between them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum CloudProvider {
    ElevenLabs,
    Microsoft,
    Xai,
    Mistral,
}
impl CloudProvider {
    pub fn from_id(id: &str) -> Option<Self> {
        match id {
            "elevenlabs" => Some(Self::ElevenLabs),
            "microsoft" => Some(Self::Microsoft),
            "xai" => Some(Self::Xai),
            "mistral" => Some(Self::Mistral),
            _ => None,
        }
    }
    /// Also the keyring `provider` attribute value.
    pub fn id(self) -> &'static str {
        match self {
            Self::ElevenLabs => "elevenlabs",
            Self::Microsoft => "microsoft",
            Self::Xai => "xai",
            Self::Mistral => "mistral",
        }
    }
    pub fn name(self) -> &'static str {
        match self {
            Self::ElevenLabs => "ElevenLabs",
            Self::Microsoft => "Microsoft Azure Speech",
            Self::Xai => "xAI",
            Self::Mistral => "Mistral",
        }
    }
    pub fn key_variables(self) -> &'static [&'static str] {
        match self {
            Self::ElevenLabs => SPEECH_KEY_VARIABLES,
            Self::Microsoft => MICROSOFT_KEY_VARIABLES,
            Self::Xai => XAI_KEY_VARIABLES,
            Self::Mistral => MISTRAL_KEY_VARIABLES,
        }
    }
    fn keyring_label(self) -> &'static str {
        match self {
            Self::ElevenLabs => "Mluva ElevenLabs API key",
            Self::Microsoft => "Mluva Microsoft Azure Speech key",
            Self::Xai => "Mluva xAI API key",
            Self::Mistral => "Mluva Mistral API key",
        }
    }
    fn unavailable(self) -> String {
        format!(
            "Desktop keyring unavailable. Set {} in your launch environment instead.",
            self.key_variables()[0]
        )
    }
    fn missing(self) -> String {
        format!(
            "{} credential unavailable. Set {} in Mluva's process environment through a secret manager or session service.",
            self.name(),
            self.key_variables()[0]
        )
    }
}

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
    Err(ProviderError(CloudProvider::ElevenLabs.missing()))
}

/// One application-owned cache, including an unavailable/empty lookup. Store
/// completion invalidates it; failed saves retain the previous cached value.
#[derive(Default)]
pub struct CredentialStore {
    cached: Mutex<HashMap<CloudProvider, Option<Secret>>>,
}
impl CredentialStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub async fn stored_speech_key(&self) -> Result<Option<Secret>> {
        self.stored_key(CloudProvider::ElevenLabs).await
    }

    pub async fn stored_key(&self, provider: CloudProvider) -> Result<Option<Secret>> {
        let mut cache = self.cached.lock().await;
        if let Some(value) = cache.get(&provider) {
            return Ok(value.clone());
        }
        let output = run_tool(
            &["lookup", "application", "mluva", "provider", provider.id()],
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
        cache.insert(provider, value.clone());
        Ok(value)
    }

    pub async fn elevenlabs_api_key(&self) -> Result<Secret> {
        self.api_key(CloudProvider::ElevenLabs).await
    }

    /// The desktop-keyring key first, then the provider's environment aliases in order.
    pub async fn api_key(&self, provider: CloudProvider) -> Result<Secret> {
        if let Some(key) = self.stored_key(provider).await? {
            return Ok(key);
        }
        for name in provider.key_variables() {
            if let Some(value) = std::env::var_os(name) {
                let value = value.into_string().map_err(|_| {
                    ProviderError(format!(
                        "{} credential contains invalid text.",
                        provider.name()
                    ))
                })?;
                if !value.is_empty() && !value.chars().all(text::whitespace) {
                    return Ok(Secret::new(value));
                }
            }
        }
        Err(ProviderError(provider.missing()))
    }

    pub async fn store_speech_key(&self, key: &str) -> Result<()> {
        self.store_key(CloudProvider::ElevenLabs, key).await
    }

    pub async fn store_key(&self, provider: CloudProvider, key: &str) -> Result<()> {
        validate_speech_key(key)?;
        let unavailable = || ProviderError(provider.unavailable());
        let output = run_tool(
            &[
                "store",
                &format!("--label={}", provider.keyring_label()),
                "application",
                "mluva",
                "provider",
                provider.id(),
            ],
            Some(key.as_bytes()),
            Duration::from_secs(60),
        )
        .await
        .ok_or_else(unavailable)?;
        // The text-mode reference decodes both captured streams, even when
        // their contents are not used. Never include those bytes in an error.
        std::str::from_utf8(&output.stdout)
            .and_then(|_| std::str::from_utf8(&output.stderr))
            .map_err(|_| ProviderError::message(INVALID_TEXT))?;
        if !output.status.success() {
            return Err(unavailable());
        }
        self.cached.lock().await.remove(&provider);
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
