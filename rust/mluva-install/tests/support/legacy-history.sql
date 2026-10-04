-- Released history schema, including unrelated fields that migration must retain.
CREATE TABLE transcription_history (
    identifier TEXT PRIMARY KEY, created_at TEXT NOT NULL,
    raw_text TEXT NOT NULL, delivered_text TEXT NOT NULL,
    mode TEXT NOT NULL, language_code TEXT NOT NULL,
    transcription_id TEXT, delivery_outcome TEXT NOT NULL,
    title TEXT, retained_audio_path TEXT, audio_retention_policy TEXT,
    application_identifier TEXT, correction_source_text TEXT,
    recognition_route TEXT, recognition_fallback_reason TEXT,
    enhancement_provider_id TEXT, enhancement_model_identifier TEXT,
    enhancement_context_sources TEXT, enhancement_outcome TEXT,
    recognition_ms INTEGER, enhancement_ms INTEGER, delivery_ms INTEGER,
    title_revision INTEGER NOT NULL DEFAULT 0
);
CREATE TABLE recording_continuations (
    segment_identifier TEXT PRIMARY KEY REFERENCES transcription_history(identifier),
    history_identifier TEXT NOT NULL REFERENCES transcription_history(identifier)
);
CREATE INDEX recording_continuations_parent ON recording_continuations(history_identifier);
CREATE TRIGGER erase_recording_continuations AFTER DELETE ON transcription_history BEGIN
    DELETE FROM recording_continuations WHERE segment_identifier = OLD.identifier OR history_identifier = OLD.identifier;
END;
CREATE TABLE unrelated (content TEXT);
INSERT INTO unrelated VALUES ('voice-scribe must stay verbatim here');
