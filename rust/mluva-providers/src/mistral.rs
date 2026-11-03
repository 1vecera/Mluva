//! Mistral Voxtral 3: batch uploads and realtime streaming over one API key.
//!
//! Batch posts the finished recording to `/v1/audio/transcriptions` with the Voxtral Small model.
//! Realtime streams base64 PCM over a WebSocket with the Voxtral Mini realtime model: the service
//! sends text deltas, which only preview, and finally one message with the complete text, which is
//! the only text that becomes committed.

use crate::compatible::Requests;
use crate::realtime::{CommittedCallback, PreviewCallback, RealtimeOptions};
use crate::stream::{self, StreamEvent, StreamProtocol, StreamSession};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, languages, multipart, transport,
};
use base64::{Engine, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::path::Path;
use std::sync::Arc;
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

pub const BATCH_ENDPOINT: &str = "https://api.mistral.ai/v1/audio/transcriptions";
pub const REALTIME_ENDPOINT: &str = "wss://api.mistral.ai/v1/audio/transcriptions/realtime";
/// The more accurate of the two batch models, at $0.24 an audio hour against Mini's $0.21.
pub const BATCH_MODEL: &str = "voxtral-small-transcribe-3";
pub const REALTIME_MODEL: &str = "voxtral-mini-transcribe-realtime-3";
const MAX_HTTP_BYTES: usize = 8_000_000;
const INVALID: &str = "Mistral returned an invalid transcription response.";
const UNREACHABLE: &str = "Mistral transcription could not reach the service.";

pub struct MistralClient {
    api_key: Secret,
    endpoint: String,
    client: reqwest::Client,
    requests: Requests,
}

impl MistralClient {
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

    /// Transcribe a finished recording. The entry pay-as-you-go tier answers bursts with HTTP 429;
    /// a few short, bounded waits honor its `Retry-After` before giving up.
    pub async fn transcribe(&self, path: &Path, language: &str) -> Result<TranscriptionResult> {
        let mut fields = vec![("model", BATCH_MODEL)];
        if language != "auto" {
            fields.push(("language", languages::iso(language)));
        }
        let boundary = format!("MluvaBoundary{}", uuid::Uuid::new_v4().simple());
        let body = multipart::encode(&fields, path, &boundary)?;
        let cancel = self.requests.begin()?;
        let response = transport::send_waiting_out_rate_limits(
            || {
                Ok(self
                    .client
                    .post(&self.endpoint)
                    .body(body.clone())
                    .header(
                        "Content-Type",
                        format!("multipart/form-data; boundary={boundary}"),
                    )
                    .header(
                        "Authorization",
                        transport::secret_header(&Secret::new(format!(
                            "Bearer {}",
                            self.api_key.value()
                        )))
                        .map_err(|_| ProviderError::message(UNREACHABLE))?,
                    )
                    .header("User-Agent", USER_AGENT))
            },
            &cancel,
            UNREACHABLE,
        )
        .await?;
        let status = response.status().as_u16();
        if !response.status().is_success() {
            return Err(ProviderError(match status {
                401 | 403 => "Mistral rejected the API key.".into(),
                429 => "Mistral transcription is rate limited. The entry tier allows only short bursts; try again shortly.".into(),
                status => format!("Mistral transcription failed with HTTP {status}."),
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
        audio_duration_seconds: None,
        words: vec![],
    })
}

/// Opens streaming sessions against the Mistral realtime endpoint.
pub struct MistralRealtimeClient {
    api_key: Secret,
    endpoint: String,
    options: RealtimeOptions,
}

impl MistralRealtimeClient {
    pub fn new(api_key: Secret, endpoint: &str, options: RealtimeOptions) -> Result<Self> {
        let parsed = url::Url::parse(endpoint).map_err(|_| {
            ProviderError::message("The Mistral realtime endpoint must be a ws:// or wss:// URL.")
        })?;
        if !matches!(parsed.scheme(), "ws" | "wss")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(ProviderError::message(
                "The Mistral realtime endpoint must be a ws:// or wss:// URL without credentials.",
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
        let unreachable = || {
            ProviderError::message("Mistral realtime transcription could not reach the service.")
        };
        let uri = realtime_uri(&self.endpoint)?;
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
            Arc::new(VoxtralProtocol::new(language)),
            language,
            self.options.clone(),
            on_preview,
            on_committed,
        )
        .await
    }
}

/// The endpoint with the realtime model; a configured query is replaced, not extended.
pub fn realtime_uri(endpoint: &str) -> Result<String> {
    let mut url = url::Url::parse(endpoint).map_err(|_| {
        ProviderError::message("The Mistral realtime endpoint must be a ws:// or wss:// URL.")
    })?;
    url.query_pairs_mut()
        .clear()
        .append_pair("model", REALTIME_MODEL);
    Ok(url.into())
}

/// Voxtral's realtime protocol: the service opens with `session.created`, the client then sets
/// the audio format and language, appends base64 PCM and ends the audio. Text arrives as deltas
/// and the turn finishes with one message carrying the complete `text` and its `usage`.
pub(crate) struct VoxtralProtocol {
    language: Option<String>,
}

impl VoxtralProtocol {
    pub(crate) fn new(language: &str) -> Self {
        Self {
            language: (language != "auto").then(|| languages::iso(language).to_owned()),
        }
    }
}

fn text_frame(value: Value) -> Message {
    Message::Text(value.to_string().into())
}

impl StreamProtocol for VoxtralProtocol {
    fn name(&self) -> &'static str {
        "Mistral"
    }
    fn waits_for_ready(&self) -> bool {
        true
    }
    fn opening(&self) -> Vec<Message> {
        let mut session = json!({"audio_format": {"encoding": "pcm_s16le", "sample_rate": 16000}});
        if let Some(language) = &self.language {
            session["language"] = json!(language);
        }
        vec![text_frame(
            json!({"type": "session.update", "session": session}),
        )]
    }
    fn audio(&self, pcm: &[u8], _first: bool) -> Vec<Message> {
        vec![text_frame(
            json!({"type": "input_audio.append", "audio": STANDARD.encode(pcm)}),
        )]
    }
    fn end(&self) -> Vec<Message> {
        vec![text_frame(json!({"type": "input_audio.end"}))]
    }
    fn decode(&self, message: &Message) -> Vec<StreamEvent> {
        let Message::Text(text) = message else {
            return vec![];
        };
        let Ok(event) = serde_json::from_str::<Value>(text) else {
            return vec![StreamEvent::Failed(
                "Mistral returned an invalid realtime transcription event.".into(),
            )];
        };
        let complete = || {
            event
                .get("text")
                .and_then(Value::as_str)
                .map(|text| vec![StreamEvent::Final(text.into())])
                .unwrap_or_else(|| {
                    vec![StreamEvent::Failed(
                        "Mistral returned an invalid realtime transcription event.".into(),
                    )]
                })
        };
        match event.get("type").and_then(Value::as_str) {
            Some("session.created") => vec![StreamEvent::Ready],
            Some("transcription.text.delta") => event
                .get("text")
                .and_then(Value::as_str)
                // Deltas carry their own leading spaces; they are appended as they are.
                .map(|delta| vec![StreamEvent::Append(delta.into())])
                .unwrap_or_default(),
            Some("transcription.done") => complete(),
            Some("error") => vec![StreamEvent::Failed(
                "Mistral realtime transcription reported an error.".into(),
            )],
            // Whatever its type, the message with the full text and its usage ends the turn.
            _ if event.get("text").is_some_and(Value::is_string)
                && event.get("usage").is_some() =>
            {
                complete()
            }
            _ => vec![],
        }
    }
}
