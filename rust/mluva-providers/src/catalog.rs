//! Content-free provider discovery; edited routes never share clients or results.
use crate::{
    Result, compatible::CompatibleClient, credentials::SPEECH_KEY_VARIABLES, models::Model,
};
use mluva_core::text;
use std::time::Duration;

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
pub fn connection_hint(provider: &str, key_env: &str) -> &'static str {
    match provider {
        "local" => "Models are managed by Mluva. No other dictation app is required.",
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
    pub base_url: String,
    pub key_env: String,
}
pub struct CatalogClient(CompatibleClient);
impl CatalogRequest {
    pub fn client(&self) -> Result<CatalogClient> {
        Ok(CatalogClient(CompatibleClient::new(
            &self.base_url,
            self.key_env.clone(),
            None,
            Duration::from_secs(5),
        )?))
    }
    pub fn message(&self, models: Option<&[Model]>) -> String {
        match models {
            None => "Could not load models. Check the endpoint (including /v1 if needed) and key variable. You can still enter a deployment ID; your choice is kept.".into(),
            Some([]) => "No matching models listed. Enter a model ID or check the server’s deployment configuration.".into(),
            Some(models) => format!("{} model(s) listed. Listing a model does not verify that your account can run it.", models.len()),
        }
    }
}
impl CatalogClient {
    pub async fn read(&self) -> Result<Vec<Model>> {
        self.0.list_models().await
    }
    pub fn cancel(&self) {
        self.0.cancel();
    }
    pub async fn close(&self) {
        self.0.close();
    }
}
