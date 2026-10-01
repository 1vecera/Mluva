//! Select only the requested speech transport; local capture needs no cloud credential.

use crate::{
    Result, TranscriptionResult,
    compatible::CompatibleClient,
    credentials::CredentialStore,
    elevenlabs::{ElevenLabsClient, SCRIBE_ENDPOINT},
    local::LocalSpeechClient,
    local_asr::OnnxOptions,
};
use mluva_core::config::AppConfig;
use std::{path::Path, time::Duration};

pub enum SpeechClient {
    ElevenLabs(ElevenLabsClient),
    Compatible(CompatibleClient),
    Local(LocalSpeechClient),
}

impl SpeechClient {
    pub async fn new(
        config: &AppConfig,
        local_options: OnnxOptions,
        credentials: &CredentialStore,
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
            _ => Ok(Self::ElevenLabs(ElevenLabsClient::new(
                credentials.elevenlabs_api_key().await?,
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
            Self::Compatible(client) => client.transcribe(path, language, model).await,
            Self::Local(client) => client.transcribe(path, language, None).await,
        }
    }

    pub async fn close(&self) {
        match self {
            Self::ElevenLabs(client) => client.close(),
            Self::Compatible(client) => client.close(),
            Self::Local(client) => client.close().await,
        }
    }

    pub async fn cancel(&self) {
        match self {
            Self::ElevenLabs(client) => client.cancel(),
            Self::Compatible(client) => client.cancel(),
            Self::Local(client) => client.cancel().await,
        }
    }
}
