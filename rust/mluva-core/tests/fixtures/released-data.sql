BEGIN TRANSACTION;
CREATE TABLE conversation_rewrites (
                    identifier INTEGER PRIMARY KEY,
                    history_identifier TEXT NOT NULL REFERENCES transcription_history(identifier),
                    instruction TEXT NOT NULL,
                    text TEXT NOT NULL,
                    model TEXT NOT NULL,
                    created_at TEXT NOT NULL
                );
INSERT INTO "conversation_rewrites" VALUES(1,'00000000-0000-4000-8000-000000000001','Polish it.','First polished reply.','synthetic-model','2026-09-01T10:00:00.123456+00:00');
INSERT INTO "conversation_rewrites" VALUES(2,'00000000-0000-4000-8000-000000000002','Make it shorter.','Straße 25%.','synthetic-model','2026-09-01T10:00:00.123456+00:00');
CREATE TABLE conversation_screenshots (
                    identifier TEXT PRIMARY KEY,
                    history_identifier TEXT,
                    capture_identifier TEXT,
                    created_at TEXT NOT NULL,
                    captured_after_seconds REAL,
                    CHECK ((history_identifier IS NULL) != (capture_identifier IS NULL))
                );
INSERT INTO "conversation_screenshots" VALUES('00000000-0000-4000-8000-000000000099','00000000-0000-4000-8000-000000000002',NULL,'2026-09-01T10:00:00.123456+00:00',1.25);
CREATE TABLE conversation_sources (
                    history_identifier TEXT PRIMARY KEY REFERENCES transcription_history(identifier),
                    text TEXT NOT NULL
                );
INSERT INTO "conversation_sources" VALUES('00000000-0000-4000-8000-000000000001','Český upravený zdroj – 25%.');
CREATE TABLE recording_continuations (
                    segment_identifier TEXT PRIMARY KEY REFERENCES transcription_history(identifier),
                    history_identifier TEXT NOT NULL REFERENCES transcription_history(identifier)
                );
CREATE TABLE transcription_history (
identifier TEXT PRIMARY KEY, created_at TEXT NOT NULL, raw_text TEXT NOT NULL,
delivered_text TEXT NOT NULL, mode TEXT NOT NULL, language_code TEXT NOT NULL,
transcription_id TEXT, delivery_outcome TEXT NOT NULL, title TEXT, title_revision INTEGER NOT NULL DEFAULT 0, retained_audio_path TEXT, audio_retention_policy TEXT, application_identifier TEXT, correction_source_text TEXT, recognition_route TEXT, recognition_fallback_reason TEXT, enhancement_provider_id TEXT, enhancement_model_identifier TEXT, enhancement_context_sources TEXT, enhancement_outcome TEXT, recognition_ms INTEGER, enhancement_ms INTEGER, delivery_ms INTEGER);
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000001','2026-09-01T10:00:00.123456+00:00','Příliš žluťoučký kůň, um, hello.','Příliš žluťoučký kůň, hello.','dictation','ces',NULL,'copied',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000002','2026-09-02T10:00:00+00:00','Second source Straße, 25%.','Second delivered 25%.','dictation','eng',NULL,'copied',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000003','2026-09-03T10:00:00+00:00',' Continued source _tag. ',' Continued delivered _tag. ','dictation','eng',NULL,'pending-preview',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000004','2026-09-04T10:00:00','Command source','Command result','command','eng','synthetic-id','copied',NULL,0,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL,NULL);
CREATE INDEX recording_continuations_parent
                    ON recording_continuations(history_identifier);
CREATE TRIGGER erase_recording_continuations
                AFTER DELETE ON transcription_history BEGIN
                    DELETE FROM recording_continuations
                    WHERE segment_identifier = OLD.identifier OR history_identifier = OLD.identifier;
                END;
CREATE INDEX screenshots_history ON conversation_screenshots(history_identifier);
CREATE INDEX screenshots_capture ON conversation_screenshots(capture_identifier);
CREATE TRIGGER erase_conversation_screenshots
                AFTER DELETE ON transcription_history BEGIN
                    DELETE FROM conversation_screenshots WHERE history_identifier = OLD.identifier;
                END;
CREATE INDEX conversation_rewrites_history
                    ON conversation_rewrites(history_identifier, identifier);
CREATE TRIGGER erase_conversation_sources
                AFTER DELETE ON transcription_history BEGIN
                    DELETE FROM conversation_sources WHERE history_identifier = OLD.identifier;
                END;
CREATE TRIGGER erase_conversation_rewrites
                AFTER DELETE ON transcription_history BEGIN
                    DELETE FROM conversation_rewrites WHERE history_identifier = OLD.identifier;
                END;
COMMIT;
