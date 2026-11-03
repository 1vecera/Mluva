//! Select only the requested speech transport; local capture needs no cloud credential.

use crate::{
    ProviderError, Result, Secret, TranscriptionResult,
    compatible::CompatibleClient,
    credentials::{CloudProvider, CredentialStore},
    elevenlabs::{ElevenLabsClient, SCRIBE_ENDPOINT},
    local::LocalSpeechClient,
    local_asr::OnnxOptions,
    microsoft::{self, MicrosoftClient},
    mistral::{self, MistralClient},
    xai::{self, XaiClient},
};
use mluva_core::config::AppConfig;
use std::{path::Path, time::Duration};

pub enum SpeechClient {
    ElevenLabs(ElevenLabsClient),
    Microsoft(MicrosoftClient),
    Xai(XaiClient),
    Mistral(MistralClient),
    Compatible(CompatibleClient),
    Local(LocalSpeechClient),
}

impl SpeechClient {
    /// A recording owns fresh cancellation/model state, using the transport
    /// already validated for this settings generation.
    pub fn new_session(&self) -> Self {
        match self {
            Self::ElevenLabs(client) => Self::ElevenLabs(client.new_session()),
            Self::Microsoft(client) => Self::Microsoft(client.new_session()),
            Self::Xai(client) => Self::Xai(client.new_session()),
            Self::Mistral(client) => Self::Mistral(client.new_session()),
            Self::Compatible(client) => Self::Compatible(client.new_session()),
            Self::Local(client) => Self::Local(client.new_session()),
        }
    }

    pub async fn new(
        config: &AppConfig,
        local_options: OnnxOptions,
        credentials: &CredentialStore,
    ) -> Result<Self> {
        let key = match CloudProvider::from_id(&config.transcription_provider) {
            Some(provider) => Some(credentials.api_key(provider).await?),
            None => None,
        };
        Self::from_config(config, local_options, key)
    }

    /// Application readiness resolves the key once. Session clones retain the
    /// immutable route with independent cancellation. Local/compatible need no keyring.
    /// `speech_key` is the selected cloud provider's key.
    pub fn from_config(
        config: &AppConfig,
        local_options: OnnxOptions,
        speech_key: Option<Secret>,
    ) -> Result<Self> {
        match config.transcription_provider.as_str() {
            "local" => {
                let mut options = local_options;
                options.model = config.local_model.clone();
                options.device = config.local_device.clone();
                Ok(Self::Local(LocalSpeechClient::new(options)?))
            }
            "litellm" => Ok(Self::Compatible(CompatibleClient::new(
                &config.transcription_base_url,
                config.transcription_api_key_env.clone(),
                config.transcription_remote_model.clone(),
                Duration::from_secs(300),
            )?)),
            "microsoft" => Ok(Self::Microsoft(MicrosoftClient::new(
                speech_key.ok_or_else(|| {
                    ProviderError("A Microsoft Azure Speech key is required.".into())
                })?,
                &microsoft::batch_endpoint(&microsoft::resolve_region(&config.microsoft_region)?)?,
                Duration::from_secs(300),
            )?)),
            "xai" => Ok(Self::Xai(XaiClient::new(
                speech_key.ok_or_else(|| ProviderError("An xAI API key is required.".into()))?,
                xai::BATCH_ENDPOINT,
                Duration::from_secs(300),
            )?)),
            "mistral" => Ok(Self::Mistral(MistralClient::new(
                speech_key.ok_or_else(|| ProviderError("A Mistral API key is required.".into()))?,
                mistral::BATCH_ENDPOINT,
                Duration::from_secs(300),
            )?)),
            _ => Ok(Self::ElevenLabs(ElevenLabsClient::new(
                speech_key
                    .ok_or_else(|| ProviderError("An ElevenLabs API key is required.".into()))?,
                SCRIBE_ENDPOINT,
                Duration::from_secs(300),
            )?)),
        }
    }

    pub async fn transcribe(
        &self,
        path: &Path,
        language: &str,
        model: &str,
    ) -> Result<TranscriptionResult> {
        match self {
            Self::ElevenLabs(client) => client.transcribe(path, language, model).await,
            Self::Microsoft(client) => client.transcribe(path, language).await,
            Self::Xai(client) => client.transcribe(path, language).await,
            Self::Mistral(client) => client.transcribe(path, language).await,
            Self::Compatible(client) => client.transcribe(path, language, model).await,
            Self::Local(client) => client.transcribe(path, language, None).await,
        }
    }

    pub async fn close(&self) {
        match self {
            Self::ElevenLabs(client) => client.close(),
            Self::Microsoft(client) => client.close(),
            Self::Xai(client) => client.close(),
            Self::Mistral(client) => client.close(),
            Self::Compatible(client) => client.close(),
            Self::Local(client) => client.close().await,
        }
    }

    pub async fn cancel(&self) {
        match self {
            Self::ElevenLabs(client) => client.cancel(),
            Self::Microsoft(client) => client.cancel(),
            Self::Xai(client) => client.cancel(),
            Self::Mistral(client) => client.cancel(),
            Self::Compatible(client) => client.cancel(),
            Self::Local(client) => client.cancel().await,
        }
    }
}
