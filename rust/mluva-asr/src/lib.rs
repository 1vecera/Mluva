//! Offline inference kernels; runtimes and models must already be verified by the owner.

pub mod frontend;
pub mod onnx;
pub mod tokens;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("Invalid local model configuration.")]
    Model,
    #[error("Expected at most 25 seconds of finite 16 kHz mono audio.")]
    Audio,
    #[error("Local inference failed.")]
    Inference,
    #[error("CUDA is unavailable.")]
    Gpu,
    #[error("This local model does not support the selected language.")]
    Language,
}

pub type Result<T> = std::result::Result<T, Error>;
