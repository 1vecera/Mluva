use crate::compatible::Requests;
use crate::models::number;
use crate::{
    ProviderError, Result, Secret, SpeakerSegment, TranscriptionResult, USER_AGENT, multipart,
    transport,
};
use serde_json::Value;
use std::collections::HashMap;
use std::path::Path;
use std::sync::LazyLock;
use std::time::Duration;

pub const SCRIBE_ENDPOINT: &str = "https://api.elevenlabs.io/v1/speech-to-text";
const INVALID: &str = "ElevenLabs returned an invalid transcription response.";
const UNREACHABLE: &str = "ElevenLabs transcription could not reach the service.";

pub struct ElevenLabsClient {
    api_key: Secret,
    endpoint: String,
    client: reqwest::Client,
    requests: Requests,
}

impl ElevenLabsClient {
    pub fn new(api_key: Secret, endpoint: &str, timeout: Duration) -> Result<Self> {
        Ok(Self {
            api_key,
            endpoint: endpoint.into(),
            client: transport::client(timeout)?,
            requests: Requests::default(),
        })
    }

    pub async fn transcribe(
        &self,
        path: &Path,
        language: &str,
        model: &str,
    ) -> Result<TranscriptionResult> {
        self.transcribe_source(path, language, model, false).await
    }
    pub async fn transcribe_meeting(
        &self,
        path: &Path,
        language: &str,
        model: &str,
    ) -> Result<TranscriptionResult> {
        self.transcribe_source(path, language, model, true).await
    }

    async fn transcribe_source(
        &self,
        path: &Path,
        language: &str,
        model: &str,
        meeting: bool,
    ) -> Result<TranscriptionResult> {
        let mut fields = vec![
            ("model_id", model),
            ("timestamps_granularity", "word"),
            ("tag_audio_events", "false"),
            ("diarize", if meeting { "true" } else { "false" }),
        ];
        if language != "auto" {
            fields.push(("language_code", language));
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
                "xi-api-key",
                transport::secret_header(&self.api_key)
                    .map_err(|_| ProviderError::message(UNREACHABLE))?,
            )
            .header("User-Agent", USER_AGENT);
        let response = tokio::select! {biased; _=cancel.cancelled()=>return Err(ProviderError::message(UNREACHABLE)),value=request.send()=>value.map_err(|_|ProviderError::message(UNREACHABLE))?};
        if !response.status().is_success() {
            return Err(ProviderError(format!(
                "ElevenLabs transcription failed with HTTP {}.",
                response.status().as_u16()
            )));
        }
        let raw = transport::bounded_body(response, None, &cancel)
            .await
            .map_err(|_| ProviderError::message(UNREACHABLE))?;
        let payload: Value =
            serde_json::from_slice(&raw).map_err(|_| ProviderError::message(INVALID))?;
        let result = transcription_result(&payload, meeting)?;
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

pub fn transcription_result(
    payload: &Value,
    include_speakers: bool,
) -> Result<TranscriptionResult> {
    let text = payload
        .get("text")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::message(INVALID))?;
    let language = payload
        .get("language_code")
        .and_then(Value::as_str)
        .ok_or_else(|| ProviderError::message(INVALID))?;
    Ok(TranscriptionResult {
        text: mluva_core::text::trim(text).into(),
        language_code: language.into(),
        language_probability: payload.get("language_probability").and_then(number),
        transcription_id: payload
            .get("transcription_id")
            .and_then(Value::as_str)
            .map(str::to_owned),
        speaker_segments: if include_speakers {
            speaker_segments(payload.get("words"))
        } else {
            vec![]
        },
        audio_duration_seconds: payload
            .get("audio_duration_secs")
            .and_then(number)
            .map(|value| value.max(0.0)),
    })
}

fn speaker_segments(words: Option<&Value>) -> Vec<SpeakerSegment> {
    let Some(words) = words.and_then(Value::as_array) else {
        return vec![];
    };
    let mut segments = vec![];
    let mut speaker: Option<String> = None;
    let mut text = String::new();
    let mut started = 0.0;
    let mut ended = 0.0_f64;
    for word in words {
        if !matches!(
            word.get("type").and_then(Value::as_str),
            Some("word" | "spacing")
        ) {
            continue;
        }
        let Some(value) = word.get("text").and_then(Value::as_str) else {
            continue;
        };
        let identifier = word
            .get("speaker_id")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .or(speaker.as_deref())
            .unwrap_or("speaker_unknown")
            .to_owned();
        let start = word
            .get("start")
            .and_then(number)
            .map_or(ended, |value| value.max(0.0));
        let end = word
            .get("end")
            .and_then(number)
            .map_or(start, |value| value.max(start));
        if speaker
            .as_ref()
            .is_some_and(|previous| previous != &identifier)
        {
            append_segment(
                &mut segments,
                speaker.as_deref().unwrap(),
                &text,
                started,
                ended,
            );
            text.clear();
            started = start;
        } else if speaker.is_none() {
            started = start;
        }
        speaker = Some(identifier);
        text.push_str(value);
        ended = ended.max(end);
    }
    if let Some(speaker) = speaker {
        append_segment(&mut segments, &speaker, &text, started, ended);
    }
    segments
}

fn append_segment(
    segments: &mut Vec<SpeakerSegment>,
    speaker: &str,
    text: &str,
    start: f64,
    end: f64,
) {
    let text = mluva_core::text::trim(text);
    if !text.is_empty() {
        segments.push(SpeakerSegment {
            speaker: speaker_label(speaker),
            text: text.into(),
            started_at_seconds: start,
            ended_at_seconds: end.max(start),
        });
    }
}

static DECIMAL_VALUES: LazyLock<HashMap<char, u8>> = LazyLock::new(|| {
    serde_json::from_str(include_str!("../resources/decimal-values.json"))
        .expect("released decimal properties")
});
fn speaker_label(speaker: &str) -> String {
    let Some(digits) = speaker
        .strip_prefix("speaker_")
        .filter(|digits| !digits.is_empty())
    else {
        return speaker.into();
    };
    let Some(mut value) = digits
        .chars()
        .map(|character| DECIMAL_VALUES.get(&character).copied())
        .collect::<Option<Vec<_>>>()
    else {
        return speaker.into();
    };
    let first = value
        .iter()
        .position(|digit| *digit != 0)
        .unwrap_or(value.len() - 1);
    value.drain(..first);
    let mut carry = true;
    for digit in value.iter_mut().rev() {
        if *digit < 9 {
            *digit += 1;
            carry = false;
            break;
        } else {
            *digit = 0;
        }
    }
    if carry {
        value.insert(0, 1);
    }
    format!(
        "Speaker {}",
        value
            .into_iter()
            .map(|digit| char::from(b'0' + digit))
            .collect::<String>()
    )
}
