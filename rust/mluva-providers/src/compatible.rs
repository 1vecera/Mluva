use crate::models::{self, Model};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, languages, multipart, transport,
};
use mluva_core::screenshots::{ImageInput, image_context, validate_images};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};
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
    timeout: Duration,
    requests: Requests,
}

#[derive(Clone, Debug)]
pub struct RewriteOptions<'a> {
    pub model: Option<&'a str>,
    pub max_output_characters: usize,
    pub effort: Option<&'a str>,
    pub service_tier: Option<&'a str>,
    pub images: &'a [ImageInput],
}
impl Default for RewriteOptions<'_> {
    fn default() -> Self {
        Self {
            model: None,
            max_output_characters: 8_000,
            effort: None,
            service_tier: None,
            images: &[],
        }
    }
}

impl CompatibleClient {
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
            timeout,
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

    pub async fn list_models(&self, capability: Option<&str>) -> Result<Vec<Model>> {
        let invalid = || ProviderError::message("The provider returned an invalid model catalog.");
        let (response, cancel) = self.open("/models", None, "application/json").await?;
        let raw = transport::bounded_body(response, Some(MAX_HTTP_BYTES), &cancel)
            .await
            .map_err(|_| invalid())?;
        let payload: Value = serde_json::from_slice(&raw).map_err(|_| invalid())?;
        models::compatible_catalog(&payload, capability, self.model.as_deref())
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

    pub async fn transform(
        &self,
        prompt: &str,
        _cwd: &Path,
        options: RewriteOptions<'_>,
        mut on_delta: Option<&mut (dyn FnMut(&str) + Send)>,
    ) -> Result<String> {
        validate_images(options.images).map_err(|error| ProviderError(error.to_string()))?;
        let model = self.resolve_model(options.model)?;
        let content = if options.images.is_empty() {
            json!(prompt)
        } else {
            let mut parts =
                vec![json!({"type":"text","text":image_context(prompt,options.images)})];
            parts.extend(
                options
                    .images
                    .iter()
                    .map(|image| json!({"type":"image_url","image_url":{"url":image.data_url()}})),
            );
            json!(parts)
        };
        let mut payload =
            json!({"model":model,"messages":[{"role":"user","content":content}],"stream":true});
        if let Some(effort) = options.effort {
            payload["reasoning_effort"] = json!(effort);
        }
        // Compatible APIs do not advertise the Codex service-tier contract.
        let body = multipart::json_body(&payload);
        let started = Instant::now();
        let (response, cancel) = self
            .open("/chat/completions", Some(body), "application/json")
            .await?;
        let mut lines = transport::Lines::new(response);
        let mut events = Vec::new();
        let mut received = 0_usize;
        let mut size = 0_usize;
        let mut parts = String::new();
        let mut completed = false;
        let invalid =
            || ProviderError::message("The provider returned an invalid or interrupted rewrite.");
        while let Some(raw) = lines
            .next(MAX_HTTP_BYTES + 1, &cancel)
            .await
            .map_err(|_| invalid())?
        {
            received = received.saturating_add(raw.len());
            if received > MAX_HTTP_BYTES {
                return Err(ProviderError::message(
                    "The provider stream exceeded the response size limit.",
                ));
            }
            if self.requests.cancelled() || started.elapsed() > self.timeout {
                return Err(ProviderError::message("Rewrite cancelled or timed out."));
            }
            let line = std::str::from_utf8(&raw)
                .map_err(|_| invalid())?
                .trim_end_matches(['\r', '\n']);
            if let Some(data) = line.strip_prefix("data:") {
                events.push(
                    data.trim_start_matches(mluva_core::text::whitespace)
                        .to_owned(),
                );
            }
            if !line.is_empty() || events.is_empty() {
                continue;
            }
            let data = events.join("\n");
            events.clear();
            if data == "[DONE]" {
                break;
            }
            let event: Value = serde_json::from_str(&data).map_err(|_| invalid())?;
            if event.get("error").is_some() {
                return Err(ProviderError::message(
                    "The rewrite provider reported a failure.",
                ));
            }
            let choices = event
                .get("choices")
                .and_then(Value::as_array)
                .ok_or_else(invalid)?;
            for choice in choices {
                let index = choice.get("index").map(models::number).unwrap_or(Some(0.0));
                if index != Some(0.0) {
                    continue;
                }
                let delta = choice
                    .get("delta")
                    .and_then(Value::as_object)
                    .ok_or_else(invalid)?;
                if ["tool_calls", "function_call"]
                    .iter()
                    .any(|key| delta.get(*key).is_some_and(models::truthy))
                {
                    return Err(ProviderError::message(
                        "The provider returned a tool request instead of text.",
                    ));
                }
                let value = delta.get("content").filter(|value| models::truthy(value));
                let text = value.map_or(Ok(""), |value| value.as_str().ok_or_else(invalid))?;
                size = size.saturating_add(text.chars().count());
                if size > options.max_output_characters {
                    return Err(ProviderError::message(
                        "The rewrite exceeded the document size limit.",
                    ));
                }
                if !text.is_empty() {
                    parts.push_str(text);
                    if let Some(callback) = &mut on_delta {
                        callback(text);
                    }
                }
                if let Some(reason) = choice
                    .get("finish_reason")
                    .filter(|reason| !reason.is_null())
                {
                    if reason.as_str() != Some("stop") {
                        return Err(ProviderError::message(
                            "The provider stopped before producing a complete rewrite.",
                        ));
                    }
                    completed = true;
                }
            }
        }
        let result = mluva_core::text::trim(&parts);
        if !completed || result.is_empty() || self.requests.cancelled() {
            return Err(ProviderError::message(
                "The provider did not finish the rewrite.",
            ));
        }
        Ok(result.into())
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
        })
    }

    pub fn close(&self) {
        self.requests.close();
    }
    pub fn cancel(&self) {
        self.requests.cancel();
    }
}
