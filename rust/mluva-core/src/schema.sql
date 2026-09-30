CREATE TABLE IF NOT EXISTS transcription_history (
    identifier TEXT PRIMARY KEY,
    created_at TEXT NOT NULL,
    raw_text TEXT NOT NULL,
    delivered_text TEXT NOT NULL,
    mode TEXT NOT NULL,
    language_code TEXT NOT NULL,
    transcription_id TEXT,
    delivery_outcome TEXT NOT NULL,
    title TEXT,
    retained_audio_path TEXT,
    audio_retention_policy TEXT,
    application_identifier TEXT,
    correction_source_text TEXT,
    recognition_route TEXT,
    recognition_fallback_reason TEXT,
    enhancement_provider_id TEXT,
    enhancement_model_identifier TEXT,
    enhancement_context_sources TEXT,
    enhancement_outcome TEXT,
    recognition_ms INTEGER,
    enhancement_ms INTEGER,
    delivery_ms INTEGER
);
CREATE TABLE IF NOT EXISTS recording_continuations (
    segment_identifier TEXT PRIMARY KEY REFERENCES transcription_history(identifier),
    history_identifier TEXT NOT NULL REFERENCES transcription_history(identifier)
);
CREATE INDEX IF NOT EXISTS recording_continuations_parent ON recording_continuations(history_identifier);
CREATE TRIGGER IF NOT EXISTS erase_recording_continuations AFTER DELETE ON transcription_history BEGIN
    DELETE FROM recording_continuations WHERE segment_identifier = OLD.identifier OR history_identifier = OLD.identifier;
END;
CREATE TABLE IF NOT EXISTS conversation_screenshots (
    identifier TEXT PRIMARY KEY,
    history_identifier TEXT,
    capture_identifier TEXT,
    created_at TEXT NOT NULL,
    captured_after_seconds REAL,
    CHECK ((history_identifier IS NULL) != (capture_identifier IS NULL))
);
CREATE INDEX IF NOT EXISTS screenshots_history ON conversation_screenshots(history_identifier);
CREATE INDEX IF NOT EXISTS screenshots_capture ON conversation_screenshots(capture_identifier);
CREATE TRIGGER IF NOT EXISTS erase_conversation_screenshots AFTER DELETE ON transcription_history BEGIN
    DELETE FROM conversation_screenshots WHERE history_identifier = OLD.identifier;
END;
CREATE TABLE IF NOT EXISTS conversation_rewrites (
    identifier INTEGER PRIMARY KEY AUTOINCREMENT,
    history_identifier TEXT NOT NULL REFERENCES transcription_history(identifier),
    instruction TEXT NOT NULL,
    text TEXT NOT NULL,
    model TEXT NOT NULL,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS conversation_rewrites_history ON conversation_rewrites(history_identifier, identifier);
CREATE TABLE IF NOT EXISTS conversation_sources (
    history_identifier TEXT PRIMARY KEY REFERENCES transcription_history(identifier),
    text TEXT NOT NULL
);
CREATE TRIGGER IF NOT EXISTS erase_conversation_sources AFTER DELETE ON transcription_history BEGIN
    DELETE FROM conversation_sources WHERE history_identifier = OLD.identifier;
END;
CREATE TRIGGER IF NOT EXISTS erase_conversation_rewrites AFTER DELETE ON transcription_history BEGIN
    DELETE FROM conversation_rewrites WHERE history_identifier = OLD.identifier;
END;
