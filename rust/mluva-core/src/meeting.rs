//! Compatible Meeting review records and a separate owner-only audio archive.

use crate::{
    database::{StoreResult, invalid},
    private_files::{atomic_write_private, resolve_path},
    text,
};
use caseless::default_case_fold_str as fold;
use chrono::{SecondsFormat, Utc};
use serde::Serialize;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
};
use uuid::Uuid;

pub const MAX_MEETINGS: usize = 200;
pub const MAX_MEETING_DOCUMENT_BYTES: u64 = 100_000_000;
pub const MEETING_PROVIDER: &str = "elevenlabsScribeV2";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingAudioSource {
    Microphone,
    System,
}
impl MeetingAudioSource {
    pub fn parse(value: &str) -> StoreResult<Self> {
        match value {
            "microphone" => Ok(Self::Microphone),
            "system" => Ok(Self::System),
            _ => Err(invalid(format!(
                "'{value}' is not a valid MeetingAudioSource"
            ))),
        }
    }
    pub fn label(self) -> &'static str {
        match self {
            Self::Microphone => "microphone",
            Self::System => "system",
        }
    }
}
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum MeetingRecognitionStatus {
    #[default]
    Completed,
    Failed,
}
impl MeetingRecognitionStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Completed => "completed",
            Self::Failed => "failed",
        }
    }
}
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct MeetingSpeakerSegment {
    pub speaker: String,
    pub text: String,
    #[serde(rename = "startedAt")]
    pub started_at_seconds: f64,
    #[serde(rename = "endedAt")]
    pub ended_at_seconds: f64,
}
impl MeetingSpeakerSegment {
    pub fn new(speaker: String, text: String, start: f64, end: f64) -> Self {
        let start = start.max(0.0);
        Self {
            speaker,
            text,
            started_at_seconds: start,
            ended_at_seconds: end.max(start),
        }
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize)]
pub struct MeetingInsights {
    pub summary: String,
    pub decisions: Vec<String>,
    pub action_items: Vec<String>,
}
#[derive(Clone, Debug, PartialEq)]
pub struct MeetingRecord {
    pub identifier: String,
    pub title: Option<String>,
    pub transcript: String,
    pub speakers: Vec<MeetingSpeakerSegment>,
    pub insights: MeetingInsights,
    pub timestamp: String,
    pub duration_seconds: f64,
    pub provider: String,
    pub language: String,
    pub audio_sources: Vec<MeetingAudioSource>,
    pub recording_filename: Option<String>,
    pub recognition_status: MeetingRecognitionStatus,
    pub transcription_id: Option<String>,
    pub warnings: Vec<String>,
}
impl MeetingRecord {
    pub fn new(transcript: String, duration_seconds: f64) -> Self {
        let now = Utc::now();
        Self {
            identifier: Uuid::new_v4().to_string(),
            title: None,
            transcript,
            speakers: vec![],
            insights: MeetingInsights::default(),
            timestamp: now.to_rfc3339_opts(
                if now.timestamp_subsec_micros() == 0 {
                    SecondsFormat::Secs
                } else {
                    SecondsFormat::Micros
                },
                true,
            ),
            duration_seconds: duration_seconds.max(0.0),
            provider: MEETING_PROVIDER.into(),
            language: "eng".into(),
            audio_sources: vec![],
            recording_filename: None,
            recognition_status: MeetingRecognitionStatus::Completed,
            transcription_id: None,
            warnings: vec![],
        }
    }
    pub fn normalized(mut self) -> StoreResult<Self> {
        validate_identifier(&self.identifier)?;
        self.duration_seconds = self.duration_seconds.max(0.0);
        self.title = self.title.and_then(|value| {
            let value = text::trim(&value);
            (!value.is_empty()).then(|| value.to_owned())
        });
        self.audio_sources = [MeetingAudioSource::Microphone, MeetingAudioSource::System]
            .into_iter()
            .filter(|source| self.audio_sources.contains(source))
            .collect();
        Ok(self)
    }
    pub fn renamed(&self, title: Option<String>) -> StoreResult<Self> {
        let mut value = self.clone();
        value.title = title;
        value.normalized()
    }
    pub fn document(&self) -> Value {
        json!({"id":self.identifier,"title":self.title,"transcript":self.transcript,"speakers":self.speakers,
            "insights":{"summary":self.insights.summary,"decisions":self.insights.decisions,"actionItems":self.insights.action_items},
            "timestamp":self.timestamp,"duration":self.duration_seconds,"provider":self.provider,"language":self.language,
            "audioSources":self.audio_sources,"recordingFilename":self.recording_filename,"recognitionStatus":self.recognition_status,
            "transcriptionID":self.transcription_id,"warnings":self.warnings})
    }
    pub fn decode(value: &Value) -> StoreResult<Self> {
        let value = value
            .as_object()
            .ok_or_else(|| invalid("Meeting archive must be an array of objects."))?;
        let speakers = object_list(value.get("speakers"), "speakers")?;
        let empty = serde_json::Map::new();
        let insights = match value.get("insights") {
            None => &empty,
            Some(Value::Object(value)) => value,
            _ => return Err(invalid("Meeting insights must be an object.")),
        };
        let sources = string_list(value.get("audioSources"), "audioSources")?
            .into_iter()
            .map(|source| {
                MeetingAudioSource::parse(if source == "system-audio" {
                    "system"
                } else {
                    &source
                })
            })
            .collect::<StoreResult<Vec<_>>>()?;
        let status = match value.get("recognitionStatus") {
            None => MeetingRecognitionStatus::Completed,
            Some(Value::String(value)) if value == "completed" => {
                MeetingRecognitionStatus::Completed
            }
            Some(Value::String(value)) if value == "failed" => MeetingRecognitionStatus::Failed,
            Some(Value::String(value)) => {
                return Err(invalid(format!(
                    "'{value}' is not a valid MeetingRecognitionStatus"
                )));
            }
            _ => return Err(invalid("Meeting recognitionStatus must be a string.")),
        };
        Self {
            identifier: required(value.get("id"), "id", false)?,
            title: optional(value.get("title"), "title")?,
            transcript: required(value.get("transcript"), "transcript", true)?,
            speakers: speakers
                .into_iter()
                .map(|row| {
                    Ok(MeetingSpeakerSegment::new(
                        required(row.get("speaker"), "speaker", false)?,
                        required(row.get("text"), "speaker text", false)?,
                        number(row.get("startedAt"), "startedAt")?,
                        number(row.get("endedAt"), "endedAt")?,
                    ))
                })
                .collect::<StoreResult<Vec<_>>>()?,
            insights: MeetingInsights {
                summary: match insights.get("summary") {
                    None => String::new(),
                    value => required(value, "summary", true)?,
                },
                decisions: string_list(insights.get("decisions"), "decisions")?,
                action_items: string_list(insights.get("actionItems"), "actionItems")?,
            },
            timestamp: required(value.get("timestamp"), "timestamp", false)?,
            duration_seconds: number(value.get("duration"), "duration")?,
            provider: required(value.get("provider"), "provider", false)?,
            language: required(value.get("language"), "language", false)?,
            audio_sources: sources,
            recording_filename: optional(value.get("recordingFilename"), "recordingFilename")?,
            recognition_status: status,
            transcription_id: optional(value.get("transcriptionID"), "transcriptionID")?,
            warnings: string_list(value.get("warnings"), "warnings")?,
        }
        .normalized()
    }
    pub fn markdown(&self) -> String {
        let speakers = self
            .speakers
            .iter()
            .map(|segment| {
                format!(
                    "- [{}] {}: {}",
                    meeting_timestamp(segment.started_at_seconds),
                    segment.speaker,
                    segment.text
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let sources = self
            .audio_sources
            .iter()
            .map(|source| source.label())
            .collect::<Vec<_>>()
            .join(", ");
        format!(
            "# {}\n\n- Captured: {}\n- Provider: {}\n- Language: {}\n- Audio: {}\n- Recognition: {}\n\n## Summary\n\n{}\n\n## Decisions\n\n{}\n\n## Action items\n\n{}\n\n## Speakers\n\n{}\n\n## Transcript\n\n{}\n",
            self.title.as_deref().unwrap_or("Mluva meeting"),
            self.timestamp,
            self.provider,
            self.language,
            if sources.is_empty() { "none" } else { &sources },
            self.recognition_status.label(),
            self.insights.summary,
            markdown_list(&self.insights.decisions),
            markdown_list(&self.insights.action_items),
            if speakers.is_empty() {
                "Speaker labels unavailable."
            } else {
                &speakers
            },
            self.transcript
        )
    }
}

pub fn validate_identifier(value: &str) -> StoreResult<()> {
    Uuid::parse_str(value)
        .map(|_| ())
        .map_err(|_| invalid("badly formed hexadecimal UUID string"))
}
fn required(value: Option<&Value>, label: &str, allow_empty: bool) -> StoreResult<String> {
    value
        .and_then(Value::as_str)
        .filter(|value| allow_empty || !text::trim(value).is_empty())
        .map(str::to_owned)
        .ok_or_else(|| invalid(format!("Meeting {label} must be a non-empty string.")))
}
fn optional(value: Option<&Value>, label: &str) -> StoreResult<Option<String>> {
    match value {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(value)) => Ok(Some(value.clone())),
        _ => Err(invalid(format!(
            "Meeting {label} must be a string or null."
        ))),
    }
}
fn number(value: Option<&Value>, label: &str) -> StoreResult<f64> {
    let number = value
        .filter(|value| value.is_number())
        .ok_or_else(|| invalid(format!("Meeting {label} must be a number.")))?;
    number
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| invalid(format!("Meeting {label} must be finite.")))
}
fn string_list(value: Option<&Value>, label: &str) -> StoreResult<Vec<String>> {
    match value {
        None => Ok(vec![]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_str()
                    .map(str::to_owned)
                    .ok_or_else(|| invalid(format!("Meeting {label} must be an array of strings.")))
            })
            .collect(),
        _ => Err(invalid(format!(
            "Meeting {label} must be an array of strings."
        ))),
    }
}
fn object_list<'a>(
    value: Option<&'a Value>,
    label: &str,
) -> StoreResult<Vec<&'a serde_json::Map<String, Value>>> {
    match value {
        None => Ok(vec![]),
        Some(Value::Array(values)) => values
            .iter()
            .map(|value| {
                value
                    .as_object()
                    .ok_or_else(|| invalid(format!("Meeting {label} must be an array of objects.")))
            })
            .collect(),
        _ => Err(invalid(format!(
            "Meeting {label} must be an array of objects."
        ))),
    }
}
fn markdown_list(values: &[String]) -> String {
    if values.is_empty() {
        "None recorded.".into()
    } else {
        values
            .iter()
            .map(|value| format!("- {value}"))
            .collect::<Vec<_>>()
            .join("\n")
    }
}
pub fn meeting_timestamp(seconds: f64) -> String {
    let seconds = (seconds + 0.5).max(0.0) as u64;
    format!("{}:{:02}", seconds / 60, seconds % 60)
}

/// Literal extracts only: no generative summary or implied meeting decisions.
pub fn extract_meeting_insights(transcript: &str) -> MeetingInsights {
    let mut sentences = vec![];
    let mut sentence = String::new();
    for word in transcript
        .split(text::whitespace)
        .filter(|word| !word.is_empty())
    {
        if !sentence.is_empty() {
            if sentence.ends_with(['.', '!', '?']) {
                sentences.push(std::mem::take(&mut sentence));
            } else {
                sentence.push(' ');
            }
        }
        sentence.push_str(word);
    }
    if !sentence.is_empty() {
        sentences.push(sentence);
    }
    let decisions = sentences
        .iter()
        .filter(|sentence| {
            let value = fold(sentence);
            [
                "we decided",
                "we agreed",
                "agreed to",
                "the decision",
                "decision:",
            ]
            .iter()
            .any(|marker| value.contains(marker))
        })
        .cloned()
        .collect();
    let action_items = sentences
        .iter()
        .filter(|sentence| {
            let value = fold(sentence);
            ["action item", "follow up", "follow-up", "todo", "to-do"]
                .iter()
                .any(|marker| value.contains(marker))
                || named_owner(sentence)
        })
        .cloned()
        .collect();
    MeetingInsights {
        summary: sentences
            .iter()
            .take(3)
            .cloned()
            .collect::<Vec<_>>()
            .join(" "),
        decisions,
        action_items,
    }
}
fn named_owner(value: &str) -> bool {
    let characters: Vec<_> = value.chars().collect();
    let letter = |character| {
        text::word_character(character) && !text::decimal(character) && character != '_'
    };
    for start in 0..characters.len() {
        if !letter(characters[start]) || start > 0 && text::word_character(characters[start - 1]) {
            continue;
        }
        if matches!(characters[start], 'w' | 'W')
            && characters
                .get(start + 1)
                .is_some_and(|c| matches!(c, 'e' | 'E'))
            && characters
                .get(start + 2)
                .is_none_or(|c| !text::word_character(*c))
        {
            continue;
        }
        let mut end = start + 1;
        while end < characters.len()
            && (letter(characters[end]) || matches!(characters[end], '\'' | '’' | '-'))
        {
            end += 1;
        }
        let before_space = end;
        while end < characters.len() && text::whitespace(characters[end]) {
            end += 1;
        }
        if end == before_space || end + 4 > characters.len() {
            continue;
        }
        if matches!(characters[end], 'w' | 'W')
            && matches!(characters[end + 1], 'i' | 'I' | 'ı' | 'İ')
            && matches!(characters[end + 2], 'l' | 'L')
            && matches!(characters[end + 3], 'l' | 'L')
            && characters
                .get(end + 4)
                .is_none_or(|c| !text::word_character(*c))
        {
            return true;
        }
    }
    false
}

#[derive(Debug)]
pub struct MeetingStore {
    pub path: PathBuf,
    pub recordings_directory: PathBuf,
    pub persistence_error: Option<String>,
    meetings: Vec<MeetingRecord>,
}
impl MeetingStore {
    pub fn new(path: impl Into<PathBuf>, recordings_directory: Option<PathBuf>) -> Self {
        let path = path.into();
        let recordings_directory = recordings_directory
            .unwrap_or_else(|| path.parent().unwrap_or(Path::new(".")).join("recordings"));
        let mut store = Self {
            path,
            recordings_directory,
            persistence_error: None,
            meetings: vec![],
        };
        if store.path.exists() {
            match store.load() {
                Ok(rows) => store.meetings = rows,
                Err(error) => store.persistence_error = Some(error.to_string()),
            }
        }
        store
    }
    pub fn meetings(&self) -> &[MeetingRecord] {
        &self.meetings
    }
    pub fn recent(&self, limit: usize) -> &[MeetingRecord] {
        &self.meetings[..limit.min(self.meetings.len())]
    }
    pub fn find(&self, identifier: &str) -> StoreResult<&MeetingRecord> {
        self.meetings
            .iter()
            .find(|row| row.identifier == identifier)
            .ok_or(crate::database::StoreError::NotFound)
    }
    pub fn save(&mut self, meeting: MeetingRecord) -> StoreResult<MeetingRecord> {
        let meeting = meeting.normalized()?;
        if text::trim(&meeting.transcript).is_empty() && meeting.recording_filename.is_none() {
            return Err(invalid(
                "A meeting requires either a transcript or a retained recording.",
            ));
        }
        let mut updated = self.meetings.clone();
        if let Some(index) = updated
            .iter()
            .position(|row| row.identifier == meeting.identifier)
        {
            updated[index] = meeting.clone();
        } else {
            updated.insert(0, meeting.clone());
        }
        let overflow = updated.split_off(updated.len().min(MAX_MEETINGS));
        self.persist(&updated)?;
        self.meetings = updated;
        for removed in overflow {
            self.delete_recording(removed.recording_filename.as_deref())?;
        }
        Ok(meeting)
    }
    pub fn rename(
        &mut self,
        identifier: &str,
        title: Option<String>,
    ) -> StoreResult<MeetingRecord> {
        self.save(self.find(identifier)?.renamed(title)?)
    }
    pub fn delete(&mut self, identifier: &str) -> StoreResult<()> {
        let removed = self.find(identifier)?.clone();
        let updated: Vec<_> = self
            .meetings
            .iter()
            .filter(|row| row.identifier != identifier)
            .cloned()
            .collect();
        self.persist(&updated)?;
        self.meetings = updated;
        self.delete_recording(removed.recording_filename.as_deref())
    }
    pub fn clear(&mut self) -> StoreResult<()> {
        self.persist(&[])?;
        let removed = std::mem::take(&mut self.meetings);
        for meeting in removed {
            self.delete_recording(meeting.recording_filename.as_deref())?;
        }
        Ok(())
    }
    pub fn recording_path(&self, meeting: &MeetingRecord) -> Option<PathBuf> {
        let filename = meeting
            .recording_filename
            .as_deref()
            .filter(|name| safe_recording_filename(name))?;
        let directory = resolve_path(&self.recordings_directory).ok()?;
        let candidate = resolve_path(&directory.join(filename)).ok()?;
        (candidate.parent() == Some(directory.as_path()) && candidate.is_file())
            .then_some(candidate)
    }
    pub fn archive_recording(&self, source: &Path, identifier: &str) -> StoreResult<PathBuf> {
        if !source.is_file() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "Meeting recording does not exist.",
            )
            .into());
        }
        validate_identifier(identifier)?;
        private_directory(&self.recordings_directory)?;
        let destination = self.recordings_directory.join(format!("{identifier}.wav"));
        if resolve_path(source)? == resolve_path(&destination)? {
            fs::set_permissions(&destination, fs::Permissions::from_mode(0o600))?;
            return Ok(destination);
        }
        if destination.exists() {
            return Err(std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "Meeting recording already exists.",
            )
            .into());
        }
        let temporary = tempfile::Builder::new()
            .prefix(&format!(".{identifier}.wav."))
            .suffix(".tmp")
            .tempfile_in(&self.recordings_directory)?;
        fs::copy(source, temporary.path())?;
        fs::set_permissions(temporary.path(), fs::Permissions::from_mode(0o600))?;
        temporary
            .persist(&destination)
            .map_err(|error| error.error)?;
        fs::remove_file(source)?;
        Ok(destination)
    }
    pub fn export(
        &self,
        meeting: &MeetingRecord,
        directory: &Path,
        format: &str,
    ) -> StoreResult<PathBuf> {
        private_directory(directory)?;
        let timestamp: String = meeting
            .timestamp
            .chars()
            .take(19)
            .filter(|character| *character != ':')
            .collect();
        let identifier: String = meeting.identifier.chars().take(8).collect();
        let (extension, content) = match format {
            "json" => ("json", pretty_document(meeting.document())?),
            "markdown" => ("md", meeting.markdown().into_bytes()),
            _ => return Err(invalid(format)),
        };
        let output = directory.join(format!(
            "mluva-meeting-{timestamp}-{identifier}.{extension}"
        ));
        atomic_write_private(&output, &content)?;
        Ok(output)
    }
    fn load(&self) -> StoreResult<Vec<MeetingRecord>> {
        if fs::metadata(&self.path)?.len() > MAX_MEETING_DOCUMENT_BYTES {
            return Err(invalid("Meeting archive exceeds its supported size."));
        }
        let bytes = fs::read(&self.path)?;
        let value: Value = serde_json::from_slice(&bytes)?;
        let rows = value
            .as_array()
            .filter(|rows| rows.iter().all(Value::is_object))
            .ok_or_else(|| invalid("Meeting archive must be an array of objects."))?;
        let meetings = rows
            .iter()
            .map(MeetingRecord::decode)
            .collect::<StoreResult<Vec<_>>>()?;
        if meetings.len() > MAX_MEETINGS {
            return Err(invalid(format!(
                "Meeting archive supports at most {MAX_MEETINGS} records."
            )));
        }
        fs::set_permissions(
            self.path.parent().unwrap_or(Path::new(".")),
            fs::Permissions::from_mode(0o700),
        )?;
        fs::set_permissions(&self.path, fs::Permissions::from_mode(0o600))?;
        Ok(meetings)
    }
    fn persist(&self, meetings: &[MeetingRecord]) -> StoreResult<()> {
        if let Some(error) = &self.persistence_error {
            return Err(invalid(format!(
                "Meeting changes are disabled until the malformed archive is repaired: {error}"
            )));
        }
        atomic_write_private(
            &self.path,
            &pretty_document(Value::Array(
                meetings.iter().map(MeetingRecord::document).collect(),
            ))?,
        )?;
        Ok(())
    }
    fn delete_recording(&self, filename: Option<&str>) -> StoreResult<()> {
        let Some(filename) = filename.filter(|name| safe_recording_filename(name)) else {
            return Ok(());
        };
        let directory = resolve_path(&self.recordings_directory)?;
        let candidate = resolve_path(&directory.join(filename))?;
        if candidate.parent() == Some(directory.as_path()) {
            match fs::remove_file(candidate) {
                Ok(()) => {}
                Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                Err(error) => return Err(error.into()),
            }
        }
        Ok(())
    }
}
fn safe_recording_filename(value: &str) -> bool {
    !value.is_empty()
        && !matches!(value, "." | "..")
        && Path::new(value)
            .file_name()
            .is_some_and(|name| name == value)
}
fn private_directory(path: &Path) -> std::io::Result<()> {
    fs::create_dir_all(path)?;
    fs::set_permissions(path, fs::Permissions::from_mode(0o700))
}
fn pretty_document(value: Value) -> StoreResult<Vec<u8>> {
    fn sorted(value: Value) -> Value {
        match value {
            Value::Array(values) => Value::Array(values.into_iter().map(sorted).collect()),
            Value::Object(values) => Value::Object(
                values
                    .into_iter()
                    .map(|(key, value)| (key, sorted(value)))
                    .collect::<BTreeMap<_, _>>()
                    .into_iter()
                    .collect(),
            ),
            value => value,
        }
    }
    let mut bytes = serde_json::to_vec_pretty(&sorted(value))?;
    bytes.push(b'\n');
    Ok(bytes)
}
