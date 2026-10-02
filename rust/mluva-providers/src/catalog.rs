//! Content-free provider discovery; edited routes never share clients or results.
use crate::{
    ProviderError, Result,
    codex::{CodexAppServerClient, CodexOptions},
    compatible::CompatibleClient,
    credentials::SPEECH_KEY_VARIABLES,
    models::{Model, valid_catalog_name, valid_identifier},
};
use mluva_core::{executables::find_executable, text};
use std::time::Duration;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Scope {
    Speech,
    Rewrite,
}
impl Scope {
    pub fn name(self) -> &'static str {
        match self {
            Self::Speech => "speech",
            Self::Rewrite => "rewrite",
        }
    }
    pub fn providers(self) -> &'static [Provider] {
        match self {
            Self::Speech => &SPEECH_PROVIDERS,
            Self::Rewrite => &REWRITE_PROVIDERS,
        }
    }
    pub fn provider_field(self) -> &'static str {
        match self {
            Self::Speech => "transcription_provider",
            Self::Rewrite => "rewrite_provider",
        }
    }
    pub fn base_field(self) -> &'static str {
        match self {
            Self::Speech => "transcription_base_url",
            Self::Rewrite => "litellm_base_url",
        }
    }
    pub fn key_field(self) -> &'static str {
        match self {
            Self::Speech => "transcription_api_key_env",
            Self::Rewrite => "litellm_api_key_env",
        }
    }
}
pub struct Provider {
    pub id: &'static str,
    pub model_field: &'static str,
    pub default_label: Option<&'static str>,
    pub description: &'static str,
}
pub const SPEECH_PROVIDERS: [Provider; 3] = [
    Provider {
        id: "elevenlabs",
        model_field: "transcription_model",
        default_label: None,
        description: "Recommended · Fast live speech recognition with Scribe v2 Realtime. About $0.39/hour before taxes; $5 of usage is about 12.8 hours at that rate. Pricing may change. I’m not affiliated with ElevenLabs in any way—just a happy user.",
    },
    Provider {
        id: "local",
        model_field: "local_model",
        default_label: None,
        description: "Mluva downloads and owns its models. Audio stays on this device. Weights load only while transcribing.",
    },
    Provider {
        id: "litellm",
        model_field: "transcription_remote_model",
        default_label: None,
        description: "Recognizes audio through your LiteLLM or OpenAI-compatible server.",
    },
];
pub const REWRITE_PROVIDERS: [Provider; 3] = [
    Provider {
        id: "codex",
        model_field: "rewrite_model",
        default_label: Some("Use Codex default"),
        description: "Uses your installed Codex app-server, sign-in and provider for inference.",
    },
    Provider {
        id: "none",
        model_field: "rewrite_model",
        default_label: Some("No rewriting"),
        description: "Polish, Live rewrite and generated titles stay off. Enable a provider later in Settings.",
    },
    Provider {
        id: "litellm",
        model_field: "litellm_model",
        default_label: None,
        description: "Uses a LiteLLM or OpenAI-compatible chat server for rewrites, cleanup and titles.",
    },
];
pub fn connection_hint(provider: &str, key_env: &str) -> &'static str {
    match provider {
        "local" => "Models are managed by Mluva. No other dictation app is required.",
        "none" => "No text is sent for rewriting. Choose a provider to enable these controls.",
        "codex" if find_executable("codex").is_some() => {
            "Codex found. Sign-in is not checked here; use codex login if needed, then refresh models."
        }
        "codex" => {
            "Install the Codex CLI and run codex login, then restart Mluva and refresh models."
        }
        _ => {
            let names = if provider == "elevenlabs" {
                SPEECH_KEY_VARIABLES
            } else {
                std::slice::from_ref(&key_env)
            };
            if names
                .iter()
                .any(|name| std::env::var(name).is_ok_and(|value| !text::trim(&value).is_empty()))
            {
                "Key found in Mluva’s environment. Account access is checked when you use the provider."
            } else if provider == "elevenlabs" {
                "Add a key in Welcome setup, or set ELEVENLABS_API_KEY through your launch environment and restart Mluva."
            } else {
                "No key found. A local server may not need one. Otherwise set the named variable in Mluva’s launch environment and restart. Connection details contain the variable name only."
            }
        }
    }
}
#[derive(Clone)]
pub struct CatalogRequest {
    pub scope: Scope,
    pub provider: String,
    pub base_url: String,
    pub key_env: String,
}
pub enum CatalogClient {
    Codex(CodexAppServerClient),
    Compatible(CompatibleClient),
}
impl CatalogRequest {
    pub fn client(&self) -> Result<CatalogClient> {
        if self.provider == "codex" {
            Ok(CatalogClient::Codex(CodexAppServerClient::new(
                CodexOptions {
                    request_timeout: Duration::from_secs(5),
                    ..Default::default()
                },
            )))
        } else {
            Ok(CatalogClient::Compatible(CompatibleClient::new(
                &self.base_url,
                self.key_env.clone(),
                None,
                Duration::from_secs(5),
            )?))
        }
    }
    pub fn message(&self, models: Option<&[Model]>) -> String {
        match models {
            None if self.provider == "codex" => "Could not load models. Check Codex sign-in, then refresh. Your choice is kept.".into(),
            None => "Could not load models. Check the endpoint (including /v1 if needed) and key variable. You can still enter a deployment ID; your choice is kept.".into(),
            Some([]) => "No matching models listed. Enter a model ID or check the server’s deployment configuration.".into(),
            Some(models) => format!("{} model(s) listed. Listing a model does not verify that your account can run it.", models.len()),
        }
    }
}
impl CatalogClient {
    pub async fn read(&self, request: &CatalogRequest) -> Result<Vec<Model>> {
        match self {
            Self::Compatible(client) => client.list_models(Some(request.scope.name())).await,
            Self::Codex(client) => {
                let models = client.list_models().await?;
                if models.iter().any(|model| {
                    !valid_identifier(&model.id)
                        || !valid_identifier(&model.identifier)
                        || !valid_catalog_name(&model.name)
                }) {
                    return Err(ProviderError("Invalid model catalog.".into()));
                }
                Ok(models)
            }
        }
    }
    pub fn cancel(&self) {
        match self {
            Self::Codex(client) => client.cancel(),
            Self::Compatible(client) => client.cancel(),
        }
    }
    pub async fn close(&self) {
        match self {
            Self::Codex(client) => client.close().await,
            Self::Compatible(client) => client.close(),
        }
    }
}
