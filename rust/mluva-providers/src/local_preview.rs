//! One on-demand local worker per recording; decoder previews are provisional.

use crate::{batch_preview::BatchPreviewSession, local_asr::OnnxOptions};
use std::path::PathBuf;

#[derive(Clone)]
pub struct LocalPreviewClient {
    pub options: OnnxOptions,
    pub directory: PathBuf,
    pub chunk_seconds: u32,
}

impl LocalPreviewClient {
    pub fn new(options: OnnxOptions, directory: PathBuf) -> Self {
        Self {
            options,
            directory,
            chunk_seconds: 3,
        }
    }

    /// Creating a capture loads no weights. Local previews stay enabled when
    /// optional text rewriting is paused; Stop reconciles and unloads the model.
    pub fn start(&self, language: &str) -> BatchPreviewSession {
        BatchPreviewSession::start_local(
            self.options.clone(),
            self.directory.clone(),
            self.chunk_seconds,
            language,
        )
    }
}
