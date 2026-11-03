//! xAI Grok Voice Transcribe 2.0: batch uploads and realtime streaming over one API key.
//!
//! Batch posts the finished recording to `/v1/stt`. Realtime sends raw PCM over a WebSocket and
//! receives growing interim text for the segment in progress and, for each finished segment, the
//! same final event twice, which the session counts once by its start time.

use crate::compatible::Requests;
use crate::realtime::{CommittedCallback, PreviewCallback, RealtimeOptions};
use crate::stream::{self, StreamEvent, StreamProtocol, StreamSession};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, languages, multipart, transport,
};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

pub const BATCH_ENDPOINT: &str = "https://api.x.ai/v1/stt";
pub const REALTIME_ENDPOINT: &str = "wss://api.x.ai/v1/stt";
pub const MODEL: &str = "grok-voice-transcribe-2.0";
const MAX_HTTP_BYTES: usize = 8_000_000;
const INVALID: &str = "xAI returned an invalid transcription response.";
const UNREACHABLE: &str = "xAI transcription could not reach the service.";

pub struct XaiClient {
    api_key: Secret,
    endpoint: String,
    client: reqwest::Client,
    requests: Requests,
}

impl XaiClient {
    /// Reuse connection/TLS setup, with independent request cancellation.
    pub fn new_session(&self) -> Self {
        Self {
            api_key: self.api_key.clone(),
            endpoint: self.endpoint.clone(),
            client: self.client.clone(),
            requests: Requests::default(),
        }
    }

    pub fn new(api_key: Secret, endpoint: &str, timeout: Duration) -> Result<Self> {
        Ok(Self {
            api_key,
            endpoint: endpoint.into(),
            client: transport::client(timeout)?,
            requests: Requests::default(),
        })
    }

    /// Transcribe a finished recording. xAI documents `file` as the last form field. Formatting
    /// (`format=true`) needs a language, so automatic detection sends neither.
    pub async fn transcribe(&self, path: &Path, language: &str) -> Result<TranscriptionResult> {
        let mut fields = vec![("model", MODEL)];
        if language != "auto" {
            fields.push(("language", languages::iso(language)));
            fields.push(("format", "true"));
        }
        let boundary = format!("MluvaBoundary{}", uuid::Uuid::new_v4().simple());
        let body = multipart::encode(&fields, path, &boundary)?;
        let cancel = self.requests.begin()?;
        let request = self
            .client
            .post(&self.endpoint)
            .body(body)
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            )
            .header(
                "Authorization",
                transport::secret_header(&Secret::new(format!("Bearer {}", self.api_key.value())))
                    .map_err(|_| ProviderError::message(UNREACHABLE))?,
            )
            .header("User-Agent", USER_AGENT);
        let response = tokio::select! {biased; _ = cancel.cancelled() => return Err(ProviderError::message(UNREACHABLE)), value = request.send() => value.map_err(|_| ProviderError::message(UNREACHABLE))?};
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ProviderError(match status {
                401 | 403 => "xAI rejected the API key.".into(),
                429 => "xAI transcription is rate limited. Try again shortly.".into(),
                status => format!("xAI transcription failed with HTTP {status}."),
            }));
        }
        let raw = transport::bounded_body(response, Some(MAX_HTTP_BYTES), &cancel)
            .await
            .map_err(|_| ProviderError::message(UNREACHABLE))?;
        let payload: Value =
            serde_json::from_slice(&raw).map_err(|_| ProviderError::message(INVALID))?;
        let result = transcription_result(&payload, language)?;
        if cancel.is_cancelled() {
            return Err(ProviderError::message(UNREACHABLE));
        }
        Ok(result)
    }

    pub fn cancel(&self) {
        self.requests.cancel();
    }
    pub fn close(&self) {
        self.requests.close();
    }
}

/// The `text` field. An empty string is a valid result with no speech, not an error.
pub fn transcription_result(payload: &Value, language: &str) -> Result<TranscriptionResult> {
    let text = payload
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::message(INVALID))?;
    let detected = payload
        .get("language")
        .and_then(Value::as_str)
        .and_then(languages::code_for_locale);
    Ok(TranscriptionResult {
        text: mluva_core::text::trim(text).into(),
        language_code: if language == "auto" {
            detected.unwrap_or("und").into()
        } else {
            language.into()
        },
        language_probability: None,
        transcription_id: None,
        speaker_segments: vec![],
        audio_duration_seconds: payload
            .get("duration")
            .and_then(Value::as_f64)
            .map(|seconds| seconds.max(0.0)),
        words: vec![],
    })
}

/// Opens streaming sessions against the xAI endpoint.
pub struct XaiRealtimeClient {
    api_key: Secret,
    endpoint: String,
    options: RealtimeOptions,
}

impl XaiRealtimeClient {
    pub fn new(api_key: Secret, endpoint: &str, options: RealtimeOptions) -> Result<Self> {
        let parsed = url::Url::parse(endpoint).map_err(|_| {
            ProviderError::message("The xAI realtime endpoint must be a ws:// or wss:// URL.")
        })?;
        if !matches!(parsed.scheme(), "ws" | "wss")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(ProviderError::message(
                "The xAI realtime endpoint must be a ws:// or wss:// URL without credentials.",
            ));
        }
        if options.session_timeout.is_zero()
            || options.finalization_timeout.is_zero()
            || options.maximum_queued_chunks == 0
        {
            return Err(ProviderError::message(
                "Realtime timeouts and the audio queue must be positive.",
            ));
        }
        Ok(Self {
            api_key,
            endpoint: endpoint.into(),
            options,
        })
    }

    pub async fn start(
        &self,
        language: &str,
        on_preview: Option<PreviewCallback>,
        on_committed: Option<CommittedCallback>,
    ) -> Result<StreamSession> {
        let unreachable =
            || ProviderError::message("xAI realtime transcription could not reach the service.");
        let uri = realtime_uri(&self.endpoint, language)?;
        let mut request = uri.into_client_request().map_err(|_| unreachable())?;
        request.headers_mut().insert(
            "Authorization",
            transport::secret_header(&Secret::new(format!("Bearer {}", self.api_key.value())))
                .map_err(|_| unreachable())?,
        );
        request
            .headers_mut()
            .insert("User-Agent", USER_AGENT.parse().unwrap());
        stream::connect(
            request,
            Arc::new(GrokProtocol::new()),
            language,
            self.options.clone(),
            on_preview,
            on_committed,
        )
        .await
    }
}

/// The stream's query: raw 16 kHz PCM, interim results and the requested language. Automatic
/// detection omits the language.
pub fn realtime_uri(endpoint: &str, language: &str) -> Result<String> {
    let mut url = url::Url::parse(endpoint).map_err(|_| {
        ProviderError::message("The xAI realtime endpoint must be a ws:// or wss:// URL.")
    })?;
    {
        let mut query = url.query_pairs_mut();
        query.clear();
        query
            .append_pair("model", MODEL)
            .append_pair("encoding", "pcm")
            .append_pair("sample_rate", "16000")
            .append_pair("interim_results", "true");
        if language != "auto" {
            query.append_pair("language", languages::iso(language));
        }
    }
    Ok(url.into())
}

pub(crate) struct GrokProtocol {
    /// Keys for finished segments that arrive without a usable start time, so they never
    /// overwrite one another or a segment that really starts at 0.
    unplaced: AtomicI64,
}

impl GrokProtocol {
    pub(crate) fn new() -> Self {
        Self {
            unplaced: AtomicI64::new(0),
        }
    }
}

impl StreamProtocol for GrokProtocol {
    fn name(&self) -> &'static str {
        "xAI"
    }
    fn waits_for_ready(&self) -> bool {
        true
    }
    fn opening(&self) -> Vec<Message> {
        vec![]
    }
    fn audio(&self, pcm: &[u8], _first: bool) -> Vec<Message> {
        vec![Message::Binary(pcm.to_vec().into())]
    }
    fn end(&self) -> Vec<Message> {
        vec![Message::Text(
            json!({"type": "audio.done"}).to_string().into(),
        )]
    }
    fn decode(&self, message: &Message) -> Vec<StreamEvent> {
        let Message::Text(text) = message else {
            return vec![];
        };
        let Ok(event) = serde_json::from_str::<Value>(text) else {
            return vec![StreamEvent::Failed(
                "xAI returned an invalid realtime transcription event.".into(),
            )];
        };
        match event.get("type").and_then(Value::as_str) {
            Some("transcript.created") => vec![StreamEvent::Ready],
            Some("transcript.partial") => {
                let Some(text) = event.get("text").and_then(Value::as_str) else {
                    return vec![StreamEvent::Failed(
                        "xAI returned an invalid realtime transcription event.".into(),
                    )];
                };
                let flag = |name: &str| event.get(name).and_then(Value::as_bool) == Some(true);
                if flag("is_final") || flag("speech_final") {
                    // The same finished segment arrives twice; its start time identifies it. Without
                    // one, a segment is its own: a repeat of it would then count twice.
                    let key = event
                        .get("start")
                        .and_then(Value::as_f64)
                        .filter(|start| start.is_finite())
                        .map(|start| (start * 1000.0).round() as i64)
                        .unwrap_or_else(|| -1 - self.unplaced.fetch_add(1, Ordering::Relaxed));
                    vec![StreamEvent::Segment {
                        key,
                        text: text.into(),
                    }]
                } else {
                    vec![StreamEvent::Interim(text.into())]
                }
            }
            Some("transcript.done") => vec![StreamEvent::Done],
            Some("error") => vec![StreamEvent::Failed(
                "xAI realtime transcription reported an error.".into(),
            )],
            _ => vec![],
        }
    }
}
