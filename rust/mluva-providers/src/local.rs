//! One native local-speech factory for the three authorized managed models.
use crate::{
    Result, TranscriptionResult,
    local_asr::{OnnxOptions, OnnxSpeechClient},
    qwen::{QwenOptions, QwenSpeechClient},
};
use std::path::Path;

pub enum LocalSpeechClient {
    Qwen(QwenSpeechClient),
    Onnx(OnnxSpeechClient),
}
impl LocalSpeechClient {
    pub fn new(options: OnnxOptions) -> Result<Self> {
        crate::local_assets::model(&options.model)?;
        if options.model == "qwen3-1.7b" {
            Ok(Self::Qwen(QwenSpeechClient::new(QwenOptions {
                data_dir: options.data_dir,
                model: options.model,
                device: options.device,
                keep_alive: options.keep_alive,
            })?))
        } else {
            Ok(Self::Onnx(OnnxSpeechClient::new(options)))
        }
    }
    pub async fn transcribe(
        &self,
        path: &Path,
        language: &str,
        on_partial: Option<&mut (dyn FnMut(String) + Send)>,
    ) -> Result<TranscriptionResult> {
        match self {
            Self::Qwen(client) => client.transcribe(path, language, on_partial).await,
            Self::Onnx(client) => client.transcribe(path, language).await,
        }
    }
    pub async fn close(&self) {
        match self {
            Self::Qwen(client) => client.close().await,
            Self::Onnx(client) => client.close().await,
        }
    }
    pub fn cancel(&self) -> impl std::future::Future<Output = ()> + '_ {
        match self {
            Self::Qwen(client) => drop(client.cancel()),
            Self::Onnx(client) => drop(client.cancel()),
        };
        self.close()
    }
}
