//! Microsoft MAI-Transcribe-2 (preview) through Azure Speech: batch uploads and realtime streaming.
//!
//! Both routes take only an Azure Speech key and region, with no SDK. Batch posts the finished
//! recording to the fast-transcription endpoint; realtime speaks the Speech service's WebSocket
//! protocol with the `mai-transcribe-2-streaming` model.

use crate::compatible::Requests;
use crate::credentials::MICROSOFT_REGION_VARIABLE;
use crate::realtime::{CommittedCallback, PreviewCallback, RealtimeOptions};
use crate::stream::{self, StreamEvent, StreamProtocol, StreamSession};
use crate::{
    ProviderError, Result, Secret, TranscriptionResult, USER_AGENT, languages, multipart, transport,
};
use chrono::Utc;
use mluva_core::config::valid_azure_region;
use serde_json::{Value, json};
use std::path::Path;
use std::sync::{
    Arc,
    atomic::{AtomicI64, Ordering},
};
use std::time::Duration;
use tokio_tungstenite::tungstenite::{Message, client::IntoClientRequest};

pub const BATCH_API_VERSION: &str = "2025-10-15";
pub const BATCH_MODEL: &str = "MAI-Transcribe-2";
pub const REALTIME_MODEL: &str = "mai-transcribe-2-streaming";
/// Regions that serve the streaming model, per Microsoft's documentation in October 2026.
pub const REALTIME_REGIONS: [&str; 4] = ["swedencentral", "centralus", "eastus2", "southeastasia"];
const MAX_HTTP_BYTES: usize = 8_000_000;
const INVALID: &str = "Microsoft returned an invalid transcription response.";
const UNREACHABLE: &str = "Microsoft transcription could not reach the service.";
const MISSING_REGION: &str = "Enter your Azure Speech region in Settings → Providers, or set AZURE_SPEECH_REGION in Mluva's launch environment.";

/// The configured region, else the `AZURE_SPEECH_REGION` environment alias. Never a default: a
/// guessed region fails with an authentication error that hides the real cause.
pub fn resolve_region(configured: &str) -> Result<String> {
    let region = if configured.is_empty() {
        std::env::var(MICROSOFT_REGION_VARIABLE).unwrap_or_default()
    } else {
        configured.to_owned()
    };
    if region.is_empty() {
        return Err(ProviderError::message(MISSING_REGION));
    }
    if !valid_azure_region(&region) {
        return Err(ProviderError::message(
            "The Azure Speech region must be a name such as swedencentral.",
        ));
    }
    Ok(region)
}

/// Fast-transcription endpoint for the region, validated so it cannot name another host.
pub fn batch_endpoint(region: &str) -> Result<String> {
    if !valid_azure_region(region) {
        return Err(ProviderError::message(
            "The Azure Speech region must be a name such as swedencentral.",
        ));
    }
    Ok(format!(
        "https://{region}.api.cognitive.microsoft.com/speechtotext/transcriptions:transcribe?api-version={BATCH_API_VERSION}"
    ))
}

/// Streaming endpoint for the region.
pub fn realtime_endpoint(region: &str) -> Result<String> {
    if !valid_azure_region(region) {
        return Err(ProviderError::message(
            "The Azure Speech region must be a name such as swedencentral.",
        ));
    }
    Ok(format!(
        "wss://{region}.stt.speech.microsoft.com/stt/speech/universal/v2"
    ))
}

pub struct MicrosoftClient {
    api_key: Secret,
    endpoint: String,
    client: reqwest::Client,
    requests: Requests,
}

impl MicrosoftClient {
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

    /// Transcribe a finished recording. The free tier answers bursts with HTTP 429; a few short,
    /// bounded waits honor its `Retry-After` before giving up.
    pub async fn transcribe(&self, path: &Path, language: &str) -> Result<TranscriptionResult> {
        let mut definition = json!({
            "enhancedMode": {
                "enabled": true,
                "model": BATCH_MODEL,
                "modelOptions": {"transcribeStyle": "verbatim"},
            }
        });
        if language != "auto" {
            definition["locales"] = json!([languages::iso(language)]);
        }
        let definition = definition.to_string();
        let boundary = format!("MluvaBoundary{}", uuid::Uuid::new_v4().simple());
        let body = multipart::encode_named(
            &[("definition", definition.as_str())],
            "audio",
            path,
            &boundary,
        )?;
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
                        "Ocp-Apim-Subscription-Key",
                        transport::secret_header(&self.api_key)
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
                401 | 403 => "Microsoft rejected the Azure Speech key or region.".into(),
                429 => "Microsoft Azure Speech is rate limited. The free tier allows one request at a time; try again shortly.".into(),
                status => format!("Microsoft transcription failed with HTTP {status}."),
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

/// `combinedPhrases[*].text` joined with spaces. An empty list is a valid result with no speech.
pub fn transcription_result(payload: &Value, language: &str) -> Result<TranscriptionResult> {
    let phrases = payload
        .get("combinedPhrases")
        .and_then(Value::as_array)
        .ok_or_else(|| ProviderError::message(INVALID))?;
    let mut parts = vec![];
    for phrase in phrases {
        let text = phrase
            .get("text")
            .and_then(Value::as_str)
            .ok_or_else(|| ProviderError::message(INVALID))?;
        let text = mluva_core::text::trim(text);
        if !text.is_empty() {
            parts.push(text);
        }
    }
    let detected = payload
        .get("phrases")
        .and_then(Value::as_array)
        .and_then(|phrases| {
            phrases
                .iter()
                .find_map(|phrase| phrase.get("locale")?.as_str())
        })
        .and_then(languages::code_for_locale);
    Ok(TranscriptionResult {
        text: parts.join(" "),
        language_code: if language == "auto" {
            detected.unwrap_or("und").into()
        } else {
            language.into()
        },
        language_probability: None,
        transcription_id: None,
        speaker_segments: vec![],
        audio_duration_seconds: payload
            .get("durationMilliseconds")
            .and_then(Value::as_f64)
            .map(|milliseconds| (milliseconds / 1000.0).max(0.0)),
        words: vec![],
    })
}

/// Opens streaming sessions against one region's endpoint.
pub struct MicrosoftRealtimeClient {
    api_key: Secret,
    endpoint: String,
    options: RealtimeOptions,
}

impl MicrosoftRealtimeClient {
    pub fn new(api_key: Secret, endpoint: &str, options: RealtimeOptions) -> Result<Self> {
        let parsed = url::Url::parse(endpoint).map_err(|_| {
            ProviderError::message("The Microsoft realtime endpoint must be a ws:// or wss:// URL.")
        })?;
        if !matches!(parsed.scheme(), "ws" | "wss")
            || parsed.host_str().is_none()
            || !parsed.username().is_empty()
            || parsed.password().is_some()
        {
            return Err(ProviderError::message(
                "The Microsoft realtime endpoint must be a ws:// or wss:// URL without credentials.",
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
        let rejected = || {
            ProviderError::message("Microsoft rejected the credentials for realtime transcription.")
        };
        let mut request = self.endpoint.as_str().into_client_request().map_err(|_| {
            ProviderError::message("Microsoft realtime transcription could not reach the service.")
        })?;
        let headers = request.headers_mut();
        headers.insert(
            "Ocp-Apim-Subscription-Key",
            transport::secret_header(&self.api_key).map_err(|_| rejected())?,
        );
        headers.insert(
            "X-ConnectionId",
            uuid::Uuid::new_v4().simple().to_string().parse().unwrap(),
        );
        headers.insert("User-Agent", USER_AGENT.parse().unwrap());
        let protocol = Arc::new(UspProtocol::new(language));
        stream::connect(
            request,
            protocol,
            language,
            self.options.clone(),
            on_preview,
            on_committed,
        )
        .await
    }
}

/// Azure Speech's WebSocket protocol: text frames carry a `Path`/`X-RequestId`/`X-Timestamp`
/// header block, a blank line and JSON; audio frames prefix their header with its length.
pub(crate) struct UspProtocol {
    request_id: String,
    locale: Option<String>,
    /// Keys for finished phrases that arrive without an offset.
    unplaced: AtomicI64,
}

impl UspProtocol {
    pub(crate) fn new(language: &str) -> Self {
        Self {
            request_id: uuid::Uuid::new_v4().simple().to_string(),
            locale: (language != "auto").then(|| languages::locale(language).to_owned()),
            unplaced: AtomicI64::new(0),
        }
    }

    fn timestamp() -> String {
        Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
    }

    fn text_frame(&self, path: &str, body: &Value) -> Message {
        Message::Text(
            format!(
                "Path: {path}\r\nX-RequestId: {}\r\nX-Timestamp: {}\r\nContent-Type: application/json\r\n\r\n{body}",
                self.request_id,
                Self::timestamp()
            )
            .into(),
        )
    }

    fn audio_frame(&self, payload: &[u8]) -> Message {
        let header = format!(
            "Path: audio\r\nX-RequestId: {}\r\nX-Timestamp: {}\r\nContent-Type: audio/x-wav\r\n",
            self.request_id,
            Self::timestamp()
        );
        let mut frame = Vec::with_capacity(2 + header.len() + payload.len());
        frame.extend_from_slice(&(header.len() as u16).to_be_bytes());
        frame.extend_from_slice(header.as_bytes());
        frame.extend_from_slice(payload);
        Message::Binary(frame.into())
    }
}

/// RIFF/WAVE header for 16 kHz, 16-bit, mono PCM with unknown (zero) sizes, which opens the first
/// audio frame of a stream.
fn wav_header() -> Vec<u8> {
    let mut header = Vec::with_capacity(44);
    header.extend_from_slice(b"RIFF");
    header.extend_from_slice(&0u32.to_le_bytes());
    header.extend_from_slice(b"WAVEfmt ");
    header.extend_from_slice(&16u32.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&1u16.to_le_bytes());
    header.extend_from_slice(&16_000u32.to_le_bytes());
    header.extend_from_slice(&32_000u32.to_le_bytes());
    header.extend_from_slice(&2u16.to_le_bytes());
    header.extend_from_slice(&16u16.to_le_bytes());
    header.extend_from_slice(b"data");
    header.extend_from_slice(&0u32.to_le_bytes());
    header
}

impl StreamProtocol for UspProtocol {
    fn name(&self) -> &'static str {
        "Microsoft"
    }
    fn waits_for_ready(&self) -> bool {
        false
    }
    fn opening(&self) -> Vec<Message> {
        let mut phrase_detection = json!({"mode": "CONVERSATION"});
        if let Some(locale) = &self.locale {
            phrase_detection["language"] = json!(locale);
        }
        vec![
            self.text_frame(
                "speech.config",
                &json!({"context": {
                    "system": {"name": "Mluva", "version": env!("CARGO_PKG_VERSION")},
                    "audio": {"source": {
                        "type": "Stream",
                        "samplerate": "16000",
                        "bitspersample": "16",
                        "channelcount": "1",
                    }},
                }}),
            ),
            self.text_frame(
                "speech.context",
                &json!({
                    "phraseDetection": phrase_detection,
                    "model": {"name": REALTIME_MODEL},
                    "audio": {"streams": {"1": null}},
                }),
            ),
        ]
    }
    fn audio(&self, pcm: &[u8], first: bool) -> Vec<Message> {
        let mut frames = vec![];
        if first {
            frames.push(self.audio_frame(&wav_header()));
        }
        frames.push(self.audio_frame(pcm));
        frames
    }
    fn end(&self) -> Vec<Message> {
        vec![self.audio_frame(&[])]
    }
    fn decode(&self, message: &Message) -> Vec<StreamEvent> {
        let Message::Text(text) = message else {
            return vec![];
        };
        let Some((head, body)) = text.split_once("\r\n\r\n") else {
            return vec![];
        };
        let path = head
            .split("\r\n")
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.trim()
                    .eq_ignore_ascii_case("path")
                    .then(|| value.trim())
            })
            .unwrap_or("");
        let body: Value = serde_json::from_str(body).unwrap_or(Value::Null);
        match path {
            "speech.hypothesis" => body
                .get("Text")
                .and_then(Value::as_str)
                .map(|text| vec![StreamEvent::Interim(text.into())])
                .unwrap_or_default(),
            "speech.phrase" => match body.get("RecognitionStatus").and_then(Value::as_str) {
                Some("Success") => {
                    let text = body
                        .get("DisplayText")
                        .and_then(Value::as_str)
                        .unwrap_or("");
                    // Offsets are unique per phrase, so a repeated phrase replaces itself.
                    let key = body
                        .get("Offset")
                        .and_then(Value::as_i64)
                        .unwrap_or_else(|| -1 - self.unplaced.fetch_add(1, Ordering::Relaxed));
                    vec![StreamEvent::Segment {
                        key,
                        text: text.into(),
                    }]
                }
                // The end-of-turn marker carries no text.
                Some("EndOfDictation") => vec![],
                // A segment that came to nothing: its hypothesis must not stay pending.
                Some("NoMatch" | "InitialSilenceTimeout" | "BabbleTimeout") => {
                    vec![StreamEvent::Discard]
                }
                _ => vec![StreamEvent::Failed(
                    "Microsoft realtime transcription reported an error.".into(),
                )],
            },
            "turn.end" => vec![StreamEvent::Done],
            _ => vec![],
        }
    }
}
