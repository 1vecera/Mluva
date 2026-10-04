//! Readiness caches credentials/paths; captures and retries own fresh transports.

use crate::{
    meeting::{
        MeetingCompletion, MeetingError, MeetingResult, MeetingWorkflow, MeetingWorkflowResult,
    },
    meeting_session::{MeetingSession, SessionResult},
};
use mluva_audio::{capture::CaptureStorage, meeting::PipeWireMeetingRecorder};
use mluva_core::{
    config::AppConfig, database::StoreError, diagnostics::DiagnosticsStore, meeting::MeetingStore,
};
use mluva_providers::{Secret, elevenlabs::ElevenLabsClient};
use std::{
    path::PathBuf,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio_util::sync::CancellationToken;

pub struct MeetingServices {
    key: Secret,
    endpoint: String,
    recorder_executable: PathBuf,
    cleanup_executable: PathBuf,
    store: Arc<Mutex<MeetingStore>>,
    diagnostics: Option<DiagnosticsStore>,
}
impl MeetingServices {
    pub fn new(
        key: Secret,
        endpoint: String,
        recorder_executable: PathBuf,
        cleanup_executable: PathBuf,
        store: Arc<Mutex<MeetingStore>>,
    ) -> Self {
        Self {
            key,
            endpoint,
            recorder_executable,
            cleanup_executable,
            store,
            diagnostics: None,
        }
    }
    pub fn with_diagnostics(mut self, diagnostics: DiagnosticsStore) -> Self {
        self.diagnostics = Some(diagnostics);
        self
    }
    fn workflow(&self, config: AppConfig) -> MeetingResult<MeetingWorkflow> {
        let client =
            ElevenLabsClient::new(self.key.clone(), &self.endpoint, Duration::from_secs(300))
                .map_err(|error| StoreError::Invalid(error.to_string()))?;
        Ok(MeetingWorkflow {
            config,
            client: Arc::new(client),
            store: self.store.clone(),
        })
    }
    pub fn launch(
        &self,
        config: AppConfig,
        completion: MeetingCompletion,
    ) -> SessionResult<Arc<MeetingSession>> {
        let storage = if completion.incognito {
            CaptureStorage::Incognito {
                cleanup_executable: self.cleanup_executable.clone(),
                memory_root: None,
            }
        } else {
            CaptureStorage::Persistent(
                self.store
                    .lock()
                    .map_err(|_| {
                        StoreError::Invalid("Meeting archive owner is unavailable.".into())
                    })
                    .map_err(MeetingError::from)?
                    .recordings_directory
                    .clone(),
            )
        };
        let recorder = PipeWireMeetingRecorder::new(
            &self.recorder_executable,
            config.microphone_target.clone(),
            config.system_audio_target.clone(),
        );
        MeetingSession::new(
            self.workflow(config)?,
            recorder,
            storage,
            completion,
            self.diagnostics.clone(),
        )
    }
    pub fn retry(&self, config: AppConfig, identifier: String) -> MeetingResult<Arc<MeetingRetry>> {
        Ok(Arc::new(MeetingRetry {
            identifier,
            workflow: self.workflow(config)?,
            lifetime: CancellationToken::new(),
            operation: tokio::sync::Mutex::new(()),
        }))
    }
}

pub struct MeetingRetry {
    pub identifier: String,
    workflow: MeetingWorkflow,
    lifetime: CancellationToken,
    operation: tokio::sync::Mutex<()>,
}
impl MeetingRetry {
    pub async fn run(&self) -> Option<MeetingResult<MeetingWorkflowResult>> {
        let _operation = self.operation.lock().await;
        tokio::select! { biased;
            _=self.lifetime.cancelled()=>None,
            result=self.workflow.retry(&self.identifier)=>Some(result),
        }
    }
    pub async fn close(&self) {
        self.lifetime.cancel();
        self.workflow.close();
        let _operation = self.operation.lock().await;
    }
}
impl Drop for MeetingRetry {
    fn drop(&mut self) {
        self.lifetime.cancel();
        self.workflow.close();
    }
}
