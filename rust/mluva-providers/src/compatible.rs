use crate::models::{self, Model};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, languages, multipart, transport,
};
use serde_json::Value;
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio_util::sync::CancellationToken;

pub const MAX_HTTP_BYTES: usize = 2_000_000;

#[derive(Clone, Default)]
pub(crate) struct Requests {
    cancelled: CancellationToken,
    current: Arc<Mutex<Option<CancellationToken>>>,
}
impl Requests {
    pub(crate) fn begin(&self) -> Result<CancellationToken> {
        if self.cancelled.is_cancelled() {
            return Err(ProviderError::message("Request cancelled."));
        }
        let token = self.cancelled.child_token();
        *self.current.lock().unwrap() = Some(token.clone());
        Ok(token)
    }
    pub(crate) fn cancelled(&self) -> bool {
        self.cancelled.is_cancelled()
    }
    pub(crate) fn cancel(&self) {
        self.cancelled.cancel();
        self.close();
    }
    pub(crate) fn close(&self) {
        if let Some(token) = self.current.lock().unwrap().take() {
            token.cancel();
        }
    }
}

pub struct CompatibleClient {
    client: reqwest::Client,
    base_url: String,
    key_env: String,
    model: Option<String>,
    requests: Requests,
}

impl CompatibleClient {
    /// Reuse connection/TLS setup, with independent request cancellation.
    pub fn new_session(&self) -> Self {
        Self {
            client: self.client.clone(),
            base_url: self.base_url.clone(),
            key_env: self.key_env.clone(),
            model: self.model.clone(),
            requests: Requests::default(),
        }
    }

    pub fn new(
        base_url: &str,
        key_env: impl Into<String>,
        model: Option<String>,
        timeout: Duration,
    ) -> Result<Self> {
        mluva_core::config::validate_provider_url(base_url)
            .map_err(|error| ProviderError(error.to_string()))?;
        Ok(Self {
            client: transport::client(timeout)?,
            base_url: base_url.trim_end_matches('/').into(),
            key_env: key_env.into(),
            model,
            requests: Requests::default(),
        })
    }

    async fn open(
        &self,
        path: &str,
        body: Option<Vec<u8>>,
        content_type: &str,
    ) -> Result<(reqwest::Response, CancellationToken)> {
        let cancel = self.requests.begin()?;
        let mut request = match body {
            Some(body) => self
                .client
                .post(format!("{}{path}", self.base_url))
                .body(body),
            None => self.client.get(format!("{}{path}", self.base_url)),
        }
        .header("Content-Type", content_type)
        .header("Accept", "application/json")
        .header("User-Agent", USER_AGENT);
        if let Ok(key) = std::env::var(&self.key_env)
            && !key.is_empty()
        {
            request = request.header(
                "Authorization",
                transport::secret_header(&Secret::new(format!("Bearer {key}")))?,
            );
        }
        let response = tokio::select! {biased; _=cancel.cancelled()=>return Err(ProviderError::message("Request cancelled.")),value=request.send()=>value.map_err(|_|ProviderError::message("Could not connect to the configured provider."))?};
        if self.requests.cancelled() {
            return Err(ProviderError::message("Request cancelled."));
        }
        if !response.status().is_success() {
            return Err(ProviderError(format!(
                "Provider request failed (HTTP {}). Check model and credentials.",
                response.status().as_u16()
            )));
        }
        Ok((response, cancel))
    }

    pub async fn list_models(&self) -> Result<Vec<Model>> {
        let invalid = || ProviderError::message("The provider returned an invalid model catalog.");
        let (response, cancel) = self.open("/models", None, "application/json").await?;
        let raw = transport::bounded_body(response, Some(MAX_HTTP_BYTES), &cancel)
            .await
            .map_err(|_| invalid())?;
        let payload: Value = serde_json::from_slice(&raw).map_err(|_| invalid())?;
        models::compatible_catalog(&payload, self.model.as_deref())
    }

    pub fn resolve_model<'a>(&'a self, requested: Option<&'a str>) -> Result<&'a str> {
        self.model
            .as_deref()
            .filter(|model| !model.is_empty())
            .or(requested.filter(|model| !model.is_empty()))
            .ok_or_else(|| {
                ProviderError::message("Choose a LiteLLM model in Settings → Providers.")
            })
    }

    pub async fn transcribe(
        &self,
        path: &Path,
        language_code: &str,
        model_id: &str,
    ) -> Result<TranscriptionResult> {
        let mut fields = vec![
            ("model", self.resolve_model(Some(model_id))?),
            ("response_format", "json"),
        ];
        if language_code != "auto" {
            fields.push(("language", languages::iso(language_code)));
        }
        let boundary = format!("mluva-{}", uuid::Uuid::new_v4().simple());
        let body = multipart::encode(&fields, path, &boundary)?;
        let (response, cancel) = self
            .open(
                "/audio/transcriptions",
                Some(body),
                &format!("multipart/form-data; boundary={boundary}"),
            )
            .await?;
        let invalid =
            || ProviderError::message("The provider did not return a complete transcript.");
        let raw = transport::bounded_body(response, Some(MAX_HTTP_BYTES), &cancel)
            .await
            .map_err(|_| invalid())?;
        if self.requests.cancelled() {
            return Err(invalid());
        }
        let payload: Value = serde_json::from_slice(&raw).map_err(|_| invalid())?;
        let text = payload
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(invalid)?;
        Ok(TranscriptionResult {
            text: text.into(),
            language_code: language_code.into(),
            language_probability: None,
            transcription_id: None,
            speaker_segments: vec![],
            audio_duration_seconds: None,
            words: vec![],
        })
    }

    pub fn close(&self) {
        self.requests.close();
    }
    pub fn cancel(&self) {
        self.requests.cancel();
    }
}
