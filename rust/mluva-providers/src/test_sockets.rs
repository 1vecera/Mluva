//! Unix sockets for tests, in short private directories under `/tmp`. A socket path is limited
//! to 107 bytes, and Mluva refuses Claude Code inbox paths over 103, while `TMPDIR` can sit deep
//! inside a worktree; fixtures that bound sockets there tested the path's length instead of the
//! behaviour they meant to. Waits for a connection are bounded, so a transport that refuses
//! early fails its test with a message instead of hanging the run.
use std::time::Duration;
use tokio::net::{UnixListener, UnixStream};

/// How long a test peer waits for the connection the code under test should make.
pub const CONNECTION_WAIT: Duration = Duration::from_secs(10);

/// A new private (0700) directory under `/tmp`, whatever `TMPDIR` is.
pub fn socket_dir() -> tempfile::TempDir {
    tempfile::Builder::new()
        .prefix("mluva-")
        .tempdir_in("/tmp")
        .expect("a private directory under /tmp")
}

/// The next connection, or a failed test that says what never connected.
pub async fn accept_within(listener: &UnixListener, what: &str) -> UnixStream {
    match tokio::time::timeout(CONNECTION_WAIT, listener.accept()).await {
        Ok(Ok((stream, _))) => stream,
        Ok(Err(error)) => panic!("{what}: accepting failed: {error}"),
        Err(_) => panic!("{what}: no connection within {CONNECTION_WAIT:?}"),
    }
}
