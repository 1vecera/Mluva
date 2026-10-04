use futures_util::{SinkExt, StreamExt};
use native_tls::Identity;
use serde_json::{Value, json};
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::net::TcpListener;
use tokio::process::Command;
use tokio_native_tls::TlsAcceptor;
use tokio_tungstenite::tungstenite::Message;

mod support;

struct Certificate {
    directory: tempfile::TempDir,
    certificate: PathBuf,
    acceptor: TlsAcceptor,
}
fn certificate() -> Certificate {
    let directory = tempfile::tempdir().unwrap();
    let certificate = directory.path().join("cert.pem");
    let key = directory.path().join("key.pem");
    let identity = directory.path().join("identity.p12");
    let status = std::process::Command::new("openssl")
        .args([
            "req",
            "-x509",
            "-newkey",
            "rsa:2048",
            "-nodes",
            "-subj",
            "/CN=localhost",
            "-addext",
            "subjectAltName=DNS:localhost",
            "-days",
            "1",
            "-out",
        ])
        .arg(&certificate)
        .arg("-keyout")
        .arg(&key)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let status = std::process::Command::new("openssl")
        .args(["pkcs12", "-export", "-passout", "pass:synthetic", "-in"])
        .arg(&certificate)
        .arg("-inkey")
        .arg(&key)
        .arg("-out")
        .arg(&identity)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .status()
        .unwrap();
    assert!(status.success());
    let identity = Identity::from_pkcs12(&std::fs::read(identity).unwrap(), "synthetic").unwrap();
    let acceptor = TlsAcceptor::from(native_tls::TlsAcceptor::new(identity).unwrap());
    std::fs::create_dir(directory.path().join("no-system-certs")).unwrap();
    Certificate {
        directory,
        certificate,
        acceptor,
    }
}
async fn peer(mode: &str, url: &str, certificate: &Path, directory: &Path) -> Value {
    let output = tokio::time::timeout(
        Duration::from_secs(5),
        Command::new(env!("CARGO_BIN_EXE_provider-fixture-peer"))
            .env_clear()
            .env("SSL_CERT_FILE", certificate)
            .env("SSL_CERT_DIR", directory.join("no-system-certs"))
            .env("MLUVA_FIXTURE_KEY", "synthetic-wire-credential")
            .args([mode, url])
            .kill_on_drop(true)
            .output(),
    )
    .await
    .unwrap()
    .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}

// Tungstenite requires its unboxed HTTP error response in handshake callbacks.
#[allow(clippy::result_large_err)]
#[tokio::test]
async fn native_https_and_wss_verify_trust_and_hostname_in_an_isolated_process() {
    let certificate = certificate();
    for websocket in [false, true] {
        for trusted in [true, false] {
            let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
            let port = listener.local_addr().unwrap().port();
            let endpoint = format!(
                "{}://{}:{port}",
                if websocket { "wss" } else { "https" },
                if trusted { "localhost" } else { "127.0.0.1" }
            );
            let acceptor = certificate.acceptor.clone();
            let server = tokio::spawn(async move {
                let (socket, _) = listener.accept().await.unwrap();
                let result = acceptor.accept(socket).await;
                if !trusted {
                    return;
                }
                let mut socket = result.unwrap();
                if websocket {
                    let mut socket=tokio_tungstenite::accept_hdr_async(socket,|request:&tokio_tungstenite::tungstenite::handshake::server::Request,response:tokio_tungstenite::tungstenite::handshake::server::Response| {
                        assert_eq!(request.headers()["xi-api-key"],"synthetic-wire-credential");Ok(response)
                    }).await.unwrap();
                    socket
                        .send(Message::Text(
                            json!({"message_type":"session_started","session_id":"tls-session"})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    let Message::Text(message) = socket.next().await.unwrap().unwrap() else {
                        panic!("expected commit");
                    };
                    let event: Value = serde_json::from_str(&message).unwrap();
                    assert_eq!(event["commit"], true);
                    assert_eq!(event["audio_base_64"], "");
                    socket
                        .send(Message::Text(
                            json!({"message_type":"committed_transcript","text":""})
                                .to_string()
                                .into(),
                        ))
                        .await
                        .unwrap();
                    while let Some(Ok(message)) = socket.next().await {
                        if message.is_close() {
                            break;
                        }
                    }
                } else {
                    let request = support::request(&mut socket).await;
                    assert_eq!(request.path, "/models");
                    assert_eq!(
                        request.headers["authorization"],
                        "Bearer synthetic-wire-credential"
                    );
                    support::respond(
                        &mut socket,
                        &support::Response::ok(br#"{"data":[]}"#.to_vec()),
                    )
                    .await;
                }
            });
            let result = peer(
                if websocket { "realtime" } else { "catalog" },
                &endpoint,
                &certificate.certificate,
                certificate.directory.path(),
            )
            .await;
            if trusted {
                assert!(result.get("ok").is_some(), "{result}");
            } else {
                assert_eq!(
                    result["error"],
                    if websocket {
                        "ElevenLabs realtime transcription could not reach the service."
                    } else {
                        "Could not connect to the configured provider."
                    }
                );
            }
            tokio::time::timeout(Duration::from_secs(3), server)
                .await
                .unwrap()
                .unwrap();
        }
    }
    // A matching hostname with no configured trusted certificate must also be refused.
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!(
        "https://localhost:{}",
        listener.local_addr().unwrap().port()
    );
    let acceptor = certificate.acceptor.clone();
    let server = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let _ = acceptor.accept(socket).await;
    });
    let result = peer(
        "catalog",
        &url,
        &certificate.directory.path().join("absent.pem"),
        certificate.directory.path(),
    )
    .await;
    assert_eq!(
        result["error"],
        "Could not connect to the configured provider."
    );
    server.await.unwrap();
}
