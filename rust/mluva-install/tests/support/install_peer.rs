//! Unshipped dependency-load endpoint for transaction faults. Real GTK loading
//! is verified separately through the complete installed production bundle.
fn main() {
    if std::env::args().nth(1).as_deref() == Some("--leave-wal") {
        let data = std::path::PathBuf::from(std::env::args_os().nth(2).unwrap());
        let connection = rusqlite::Connection::open(data.join("history.sqlite3")).unwrap();
        connection
            .execute_batch("PRAGMA journal_mode=WAL; PRAGMA wal_autocheckpoint=0;")
            .unwrap();
        connection.execute("INSERT INTO transcription_history (identifier,created_at,raw_text,delivered_text,mode,language_code,delivery_outcome,retained_audio_path) VALUES ('wal','2026-09-13','Committed WAL voice-scribe','WAL kept','scratchpad','ces','copied',?1)", [data.join("recordings/audio.wav").to_str().unwrap()]).unwrap();
        // Deliberately omit SQLite close/checkpoint: this fixture models a
        // stopped process with committed frames awaiting the next opener.
        std::process::exit(0);
    }
    let root = std::path::PathBuf::from(std::env::var_os("MLUVA_INSTALL_PEER").unwrap());
    std::fs::write(root.join("dependency-check"), b"loaded\n").unwrap();
    if std::env::var("MLUVA_INSTALL_CASE").unwrap() == "dependency-failure" {
        std::process::exit(73);
    }
}
