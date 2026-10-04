BEGIN TRANSACTION;
CREATE TABLE transcription_history (
identifier TEXT PRIMARY KEY, created_at TEXT NOT NULL, raw_text TEXT NOT NULL,
delivered_text TEXT NOT NULL, mode TEXT NOT NULL, language_code TEXT NOT NULL,
transcription_id TEXT, delivery_outcome TEXT NOT NULL);
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000001','2026-09-01T10:00:00.123456+00:00','Příliš žluťoučký kůň, um, hello.','Příliš žluťoučký kůň, hello.','dictation','ces',NULL,'copied');
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000002','2026-09-02T10:00:00+00:00','Second source Straße, 25%.','Second delivered 25%.','dictation','eng',NULL,'copied');
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000003','2026-09-03T10:00:00+00:00',' Continued source _tag. ',' Continued delivered _tag. ','dictation','eng',NULL,'pending-preview');
INSERT INTO "transcription_history" VALUES('00000000-0000-4000-8000-000000000004','2026-09-04T10:00:00','Command source','Command result','command','eng','synthetic-id','copied');
COMMIT;
