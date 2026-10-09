//! Verify Access signatures, issuer, application audience and lifetime at the origin.

use jsonwebtoken::{Algorithm, DecodingKey, Validation, decode, decode_header, jwk::JwkSet};
use serde::Deserialize;
use std::time::{Duration, Instant};
use tokio::sync::Mutex;

pub struct Access {
    issuer: String,
    audience: String,
    client: reqwest::Client,
    keys: Mutex<(Instant, JwkSet)>,
}

#[derive(Clone, Deserialize)]
struct Claims {
    #[serde(rename = "type")]
    token_type: String,
}

impl Access {
    /// Fetch only the configured team endpoint; token contents never select a URL.
    pub async fn new(issuer: String, audience: String) -> Result<Self, String> {
        let url = reqwest::Url::parse(&issuer).map_err(|_| "Invalid Access team URL.")?;
        if url.scheme() != "https"
            || !url
                .host_str()
                .is_some_and(|host| host.ends_with(".cloudflareaccess.com"))
            || url.path() != "/"
            || url.query().is_some()
            || url.fragment().is_some()
            || !url.username().is_empty()
            || url.password().is_some()
            || url.port().is_some()
            || audience.is_empty()
        {
            return Err(
                "Use the HTTPS Cloudflare Access team URL and application audience.".into(),
            );
        }
        let issuer = issuer.trim_end_matches('/').to_owned();
        let client = reqwest::Client::builder()
            .timeout(Duration::from_secs(10))
            .redirect(reqwest::redirect::Policy::none())
            .build()
            .map_err(|_| "Could not prepare Access verification.")?;
        let keys = Self::fetch(&client, &issuer).await?;
        Ok(Self {
            issuer,
            audience,
            client,
            keys: Mutex::new((Instant::now(), keys)),
        })
    }

    async fn fetch(client: &reqwest::Client, issuer: &str) -> Result<JwkSet, String> {
        client
            .get(format!("{issuer}/cdn-cgi/access/certs"))
            .send()
            .await
            .map_err(|_| "Could not fetch Access signing keys.")?
            .error_for_status()
            .map_err(|_| "Access signing keys are unavailable.")?
            .json()
            .await
            .map_err(|_| "Invalid Access signing keys.".into())
    }

    pub async fn accepts(&self, token: &str) -> bool {
        if token.len() > 16_384 {
            return false;
        }
        let Ok(header) = decode_header(token) else {
            return false;
        };
        if header.alg != Algorithm::RS256 {
            return false;
        }
        let Some(kid) = header.kid else {
            return false;
        };
        let mut keys = self.keys.lock().await;
        // Refresh periodically, and once for an unknown rotated key. Rate-limit misses.
        if keys.0.elapsed() > Duration::from_secs(3600)
            || (keys.1.find(&kid).is_none() && keys.0.elapsed() > Duration::from_secs(60))
        {
            match Self::fetch(&self.client, &self.issuer).await {
                Ok(updated) => *keys = (Instant::now(), updated),
                Err(_) => {
                    return false;
                }
            }
        }
        let Some(jwk) = keys.1.find(&kid) else {
            return false;
        };
        let Ok(key) = DecodingKey::from_jwk(jwk) else {
            return false;
        };
        verify(token, &key, &self.issuer, &self.audience)
    }
}

fn verify(token: &str, key: &DecodingKey, issuer: &str, audience: &str) -> bool {
    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[issuer]);
    validation.set_audience(&[audience]);
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    validation.validate_nbf = true;
    validation.leeway = 5;
    decode::<Claims>(token, key, &validation).is_ok_and(|token| token.claims.token_type == "app")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{AppState, MAX_RECORDING_SECONDS, MAX_UPLOAD, router};
    use axum::{
        Router,
        body::{Body, to_bytes},
        http::{Request, StatusCode},
        routing::post,
    };
    use jsonwebtoken::{EncodingKey, Header, encode};
    use mluva_core::{
        config::{AppConfig, AppPaths},
        history::HistoryStore,
    };
    use mluva_providers::{Secret, elevenlabs::ElevenLabsClient};
    use serde_json::json;
    use std::{
        os::unix::fs::PermissionsExt,
        sync::{
            Arc,
            atomic::{AtomicUsize, Ordering},
        },
    };
    use tower::ServiceExt;

    const ISSUER: &str = "https://test.cloudflareaccess.com";
    const ORIGIN: &str = "https://recorder.example";

    fn token(claims: serde_json::Value) -> String {
        let mut header = Header::new(Algorithm::RS256);
        header.kid = Some("test-key".into());
        encode(
            &header,
            &claims,
            &EncodingKey::from_rsa_der(include_bytes!("../tests/fixtures/signing-key.der")),
        )
        .unwrap()
    }

    fn claims() -> serde_json::Value {
        json!({"iss":ISSUER, "aud":["app"], "exp":4_000_000_000_u64, "nbf":0, "type":"app"})
    }

    fn access() -> Arc<Access> {
        Arc::new(Access {
            issuer: ISSUER.into(),
            audience: "app".into(),
            client: reqwest::Client::new(),
            keys: Mutex::new((
                Instant::now(),
                serde_json::from_str(include_str!("../tests/fixtures/keys.json")).unwrap(),
            )),
        })
    }

    #[tokio::test]
    async fn origin_checks_signature_expiry_issuer_audience_and_application_type() {
        let access = access();
        assert!(access.accepts(&token(claims())).await);
        for (field, value) in [
            ("iss", json!("https://other.cloudflareaccess.com")),
            ("aud", json!(["other-app"])),
            ("exp", json!(1)),
            ("nbf", json!(4_000_000_000_u64)),
            ("type", json!("org")),
        ] {
            let mut wrong = claims();
            wrong[field] = value;
            assert!(
                !access.accepts(&token(wrong)).await,
                "accepted wrong {field}"
            );
        }
        let mut forged = token(claims());
        let index = forged.rfind('.').unwrap() + 2;
        let replacement = if forged.as_bytes()[index] == b'A' {
            "B"
        } else {
            "A"
        };
        forged.replace_range(index..index + 1, replacement);
        assert!(!access.accepts(&forged).await);
        assert!(!access.accepts("malformed").await);
    }

    struct Fixture {
        root: tempfile::TempDir,
        state: Arc<AppState>,
        calls: Arc<AtomicUsize>,
        provider_bytes: Arc<AtomicUsize>,
        peer: tokio::task::JoinHandle<()>,
    }

    impl Drop for Fixture {
        fn drop(&mut self) {
            self.peer.abort();
        }
    }

    async fn fixture(incognito: bool, provider_fails: bool) -> Fixture {
        let root = tempfile::tempdir().unwrap();
        let paths = AppPaths {
            config: root.path().join("config"),
            data: root.path().join("data"),
            runtime: root.path().join("runtime"),
        };
        if incognito {
            std::fs::create_dir_all(paths.data.join("app/bin")).unwrap();
            let cleanup = paths.data.join("app/bin/mluva-audio-cleanup");
            std::fs::write(&cleanup, "#!/bin/sh\nprintf 'ready\\n'\ncat >/dev/null\n").unwrap();
            std::fs::set_permissions(&cleanup, std::fs::Permissions::from_mode(0o700)).unwrap();
        }
        if !incognito {
            HistoryStore::new(paths.data.join("history.sqlite3"))
                .initialize()
                .unwrap();
        }
        let clipboard = root.path().join("clipboard");
        std::fs::write(
            &clipboard,
            format!(
                "#!/bin/sh\ncat > '{}'\n",
                root.path().join("copied.txt").display()
            ),
        )
        .unwrap();
        std::fs::set_permissions(&clipboard, std::fs::Permissions::from_mode(0o700)).unwrap();
        let calls = Arc::new(AtomicUsize::new(0));
        let seen = calls.clone();
        let provider_bytes = Arc::new(AtomicUsize::new(0));
        let received = provider_bytes.clone();
        let peer = Router::new()
            .route(
                "/scribe",
                post(move |body: axum::body::Bytes| {
                    let seen = seen.clone();
                    let received = received.clone();
                    async move {
                        seen.fetch_add(1, Ordering::SeqCst);
                        received.store(body.len(), Ordering::SeqCst);
                        assert!(body.windows(4).any(|window| window == b"RIFF"));
                        if provider_fails {
                            return (StatusCode::BAD_GATEWAY, axum::Json(json!({})));
                        }
                        (
                            StatusCode::OK,
                            axum::Json(
                                json!({"text":"Synthetic browser test.", "language_code":"eng"}),
                            ),
                        )
                    }
                }),
            )
            .layer(axum::extract::DefaultBodyLimit::max(256 * 1024 * 1024));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let endpoint = format!("http://{}/scribe", listener.local_addr().unwrap());
        let peer = tokio::spawn(async move {
            axum::serve(listener, peer).await.unwrap();
        });
        let speech = Arc::new(
            ElevenLabsClient::new(
                Secret::new("synthetic-test-key"),
                &endpoint,
                Duration::from_secs(10),
            )
            .unwrap(),
        );
        let config = AppConfig {
            incognito_mode: incognito,
            ..AppConfig::default()
        };
        let ffmpeg = mluva_core::executables::find_executable("ffmpeg")
            .expect("Browser audio checks require ffmpeg");
        let state = Arc::new(AppState::new(
            access(),
            ORIGIN.into(),
            config,
            paths,
            speech,
            clipboard,
            ffmpeg,
        ));
        Fixture {
            root,
            state,
            calls,
            provider_bytes,
            peer,
        }
    }

    async fn request(
        state: &Arc<AppState>,
        method: &str,
        path: &str,
        audio: Vec<u8>,
    ) -> axum::response::Response {
        router(state.clone())
            .oneshot(
                Request::builder()
                    .method(method)
                    .uri(path)
                    .header("cf-access-jwt-assertion", token(claims()))
                    .header("origin", ORIGIN)
                    .header("content-type", "audio/wav")
                    .body(Body::from(audio))
                    .unwrap(),
            )
            .await
            .unwrap()
    }

    fn wav() -> Vec<u8> {
        let mut audio = Vec::new();
        audio.extend(b"RIFF");
        audio.extend(3236_u32.to_le_bytes());
        audio.extend(b"WAVEfmt ");
        audio.extend(16_u32.to_le_bytes());
        audio.extend(1_u16.to_le_bytes());
        audio.extend(1_u16.to_le_bytes());
        audio.extend(16000_u32.to_le_bytes());
        audio.extend(32000_u32.to_le_bytes());
        audio.extend(2_u16.to_le_bytes());
        audio.extend(16_u16.to_le_bytes());
        audio.extend(b"data");
        audio.extend(3200_u32.to_le_bytes());
        audio.extend(vec![0; 3200]);
        audio
    }

    async fn wait_for_job(state: &Arc<AppState>, id: &str) -> serde_json::Value {
        for _ in 0..2000 {
            let response = request(state, "GET", &format!("/api/recordings/{id}"), vec![]).await;
            let job: serde_json::Value =
                serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                    .unwrap();
            if job["phase"] != "processing" {
                return job;
            }
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        panic!("Synthetic recording did not finish");
    }

    #[tokio::test]
    async fn two_hour_compressed_audio_reaches_the_provider_and_overlength_is_rejected() {
        let fixture = fixture(false, false).await;
        for (duration, accepted) in [
            (MAX_RECORDING_SECONDS, true),
            (MAX_RECORDING_SECONDS + 3, false),
        ] {
            let source = fixture.root.path().join(format!("{duration}.webm"));
            let generated = tokio::process::Command::new(&fixture.state.ffmpeg)
                .args([
                    "-nostdin",
                    "-v",
                    "error",
                    "-f",
                    "lavfi",
                    "-i",
                    "anullsrc=r=16000:cl=mono",
                    "-t",
                ])
                .arg(duration.to_string())
                .args([
                    "-c:a",
                    "libopus",
                    "-b:a",
                    "48k",
                    "-vbr",
                    "off",
                    "-compression_level",
                    "0",
                    "-frame_duration",
                    "60",
                ])
                .arg(&source)
                .status()
                .await
                .unwrap();
            assert!(generated.success());
            let audio = std::fs::read(&source).unwrap();
            assert!(audio.len() < MAX_UPLOAD);
            assert!(audio.len() > 20 * 1024 * 1024);
            let id = uuid::Uuid::new_v4().to_string();
            let response = router(fixture.state.clone())
                .oneshot(
                    Request::builder()
                        .method("POST")
                        .uri(format!("/api/recordings/{id}"))
                        .header("cf-access-jwt-assertion", token(claims()))
                        .header("origin", ORIGIN)
                        .header("content-type", "audio/webm")
                        .body(Body::from(audio))
                        .unwrap(),
                )
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::ACCEPTED);
            let result = wait_for_job(&fixture.state, &id).await;
            if accepted {
                assert_eq!(result["phase"], "completed");
                assert_eq!(result["copied"], true);
                assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
                // The local Scribe peer must receive all two hours of decoded PCM,
                // not a short/truncated conversion or a response-only mock.
                assert!(fixture.provider_bytes.load(Ordering::SeqCst) >= 230_400_000);
                assert_eq!(fixture.state.history().recent(10).unwrap().len(), 1);
            } else {
                assert_eq!(result["phase"], "failed");
                assert!(
                    result["message"]
                        .as_str()
                        .unwrap()
                        .contains("limited to 2 hours")
                );
                assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
                assert_eq!(fixture.state.history().recent(10).unwrap().len(), 1);
            }
        }
    }

    #[tokio::test]
    async fn all_assets_apis_and_unknown_routes_require_authentication_and_mutations_require_same_origin()
     {
        let fixture = fixture(false, false).await;
        for path in [
            "/",
            "/app.js",
            "/manifest.webmanifest",
            "/sw.js",
            "/icons/mluva-192.png",
            "/icons/mluva-512.png",
            "/api/recordings",
            "/api/recordings/id",
            "/unknown",
        ] {
            let response = router(fixture.state.clone())
                .oneshot(Request::builder().uri(path).body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::UNAUTHORIZED, "{path}");
            assert_eq!(response.headers()["cache-control"], "private, no-store");
        }
        for origin in [Some("https://other.example"), None] {
            let mut request = Request::builder()
                .method("POST")
                .uri("/api/recordings/id")
                .header("cf-access-jwt-assertion", token(claims()));
            if let Some(origin) = origin {
                request = request.header("origin", origin);
            }
            let response = router(fixture.state.clone())
                .oneshot(request.body(Body::empty()).unwrap())
                .await
                .unwrap();
            assert_eq!(response.status(), StatusCode::FORBIDDEN);
        }
        assert_eq!(
            request(&fixture.state, "GET", "/", vec![]).await.status(),
            StatusCode::OK
        );
        let id = uuid::Uuid::new_v4();
        assert_eq!(
            request(&fixture.state, "POST", "/api/recordings/invalid", wav())
                .await
                .status(),
            StatusCode::BAD_REQUEST
        );
        assert_eq!(
            request(
                &fixture.state,
                "POST",
                &format!("/api/recordings/{id}"),
                vec![0; MAX_UPLOAD + 1]
            )
            .await
            .status(),
            StatusCode::PAYLOAD_TOO_LARGE
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 0);
    }

    #[tokio::test]
    async fn audio_reaches_scribe_history_and_only_the_fake_clipboard_and_retry_survives_restart() {
        let fixture = fixture(false, false).await;
        let id = uuid::Uuid::new_v4().to_string();
        let path = format!("/api/recordings/{id}");
        assert_eq!(
            request(&fixture.state, "POST", &path, wav()).await.status(),
            StatusCode::ACCEPTED
        );
        let result = wait_for_job(&fixture.state, &id).await;
        assert_eq!(result["text"], "Synthetic browser test.");
        assert_eq!(result["copied"], true);
        assert_eq!(
            std::fs::read_to_string(fixture.root.path().join("copied.txt")).unwrap(),
            "Synthetic browser test."
        );
        let entry = fixture.state.history().recent(10).unwrap().remove(0);
        assert_eq!(entry.raw_text, "Synthetic browser test.");
        assert!(entry.retained_audio_path.is_none());
        // A fresh companion has no volatile receipts, but must not transcribe this ID again.
        let state = Arc::new(AppState::new(
            fixture.state.access.clone(),
            ORIGIN.into(),
            fixture.state.config.clone(),
            fixture.state.paths.clone(),
            fixture.state.speech.clone(),
            fixture.state.clipboard.clone(),
            fixture.state.ffmpeg.clone(),
        ));
        assert_eq!(
            request(&state, "POST", &path, wav()).await.status(),
            StatusCode::OK
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
        let response = request(&state, "GET", "/api/recordings", vec![]).await;
        let records: serde_json::Value =
            serde_json::from_slice(&to_bytes(response.into_body(), 1_000_000).await.unwrap())
                .unwrap();
        assert_eq!(records[0]["identifier"], id);
        state
            .history()
            .correct_delivered_text(&entry.identifier, "Edited on the PC.")
            .unwrap();
        assert_eq!(
            request(&state, "POST", &format!("{path}/copy"), vec![])
                .await
                .status(),
            StatusCode::OK
        );
        assert_eq!(
            std::fs::read_to_string(fixture.root.path().join("copied.txt")).unwrap(),
            "Edited on the PC."
        );
        assert_eq!(
            state.history().find(&entry.identifier).unwrap().raw_text,
            "Synthetic browser test."
        );
        assert_eq!(fixture.calls.load(Ordering::SeqCst), 1);
    }

    #[tokio::test]
    async fn failed_provider_retains_private_recovery_audio_without_copying() {
        let fixture = fixture(false, true).await;
        let id = uuid::Uuid::new_v4().to_string();
        assert_eq!(
            request(
                &fixture.state,
                "POST",
                &format!("/api/recordings/{id}"),
                wav()
            )
            .await
            .status(),
            StatusCode::ACCEPTED
        );
        assert_eq!(wait_for_job(&fixture.state, &id).await["phase"], "failed");
        assert!(!fixture.root.path().join("copied.txt").exists());
        let entry = fixture.state.history().recent(10).unwrap().remove(0);
        let audio = fixture
            .state
            .history()
            .managed_retained_audio(&entry.identifier)
            .unwrap();
        assert_eq!(
            std::fs::metadata(audio).unwrap().permissions().mode() & 0o777,
            0o600
        );
        assert_eq!(entry.delivery_outcome, "failed");
    }

    #[tokio::test]
    async fn incognito_writes_neither_history_nor_retained_audio() {
        let fixture = fixture(true, false).await;
        let id = uuid::Uuid::new_v4().to_string();
        request(
            &fixture.state,
            "POST",
            &format!("/api/recordings/{id}"),
            wav(),
        )
        .await;
        assert_eq!(
            wait_for_job(&fixture.state, &id).await["phase"],
            "completed"
        );
        assert!(!fixture.state.paths.data.join("history.sqlite3").exists());
        assert!(!fixture.state.paths.data.join("recordings").exists());
        let response = request(&fixture.state, "GET", "/api/recordings", vec![]).await;
        assert_eq!(to_bytes(response.into_body(), 100).await.unwrap(), "[]");
    }

    #[tokio::test]
    #[ignore = "Run with MLUVA_WEB_BROWSER_DRIVER and the isolated headless browser prerequisites"]
    async fn browser_record_stop_transfer_and_recovery() {
        let driver =
            std::env::var("MLUVA_WEB_BROWSER_DRIVER").expect("Set MLUVA_WEB_BROWSER_DRIVER");
        let mut fixture = fixture(false, false).await;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}", listener.local_addr().unwrap());
        Arc::get_mut(&mut fixture.state).unwrap().origin = url.clone();
        // Simulate Access's cookie-to-assertion edge for browser-owned requests
        // (manifest and worker updates do not use Playwright's page headers).
        // This is only a test peer; the production origin still requires signed JWTs.
        let assertion = token(claims());
        let app = router(fixture.state.clone()).layer(axum::middleware::from_fn(
            move |mut request: Request<Body>, next: axum::middleware::Next| {
                let assertion = assertion.clone();
                async move {
                    let authorized = request
                        .headers()
                        .get("cookie")
                        .and_then(|value| value.to_str().ok())
                        .is_some_and(|value| {
                            value
                                .split(';')
                                .any(|cookie| cookie.trim() == "mluva-test-access=authorized")
                        });
                    if authorized {
                        request
                            .headers_mut()
                            .insert("cf-access-jwt-assertion", assertion.parse().unwrap());
                    }
                    next.run(request).await
                }
            },
        ));
        let serving = tokio::spawn(async move {
            axum::serve(listener, app).await.unwrap();
        });
        let output = tokio::process::Command::new("node")
            .arg(driver)
            .env("MLUVA_TEST_URL", &url)
            .output()
            .await
            .unwrap();
        serving.abort();
        assert!(
            output.status.success(),
            "{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
        assert!(fixture.calls.load(Ordering::SeqCst) >= 1);
    }

    #[test]
    fn signature_algorithm_and_claims_are_not_trusted_without_verification() {
        // A symmetric forgery must never be accepted as an Access application token.
        let claims = json!({"iss":"https://team.cloudflareaccess.com", "aud":["app"],
            "exp":4_000_000_000_u64, "nbf":0, "type":"app"});
        let forged = encode(
            &Header::new(Algorithm::HS256),
            &claims,
            &EncodingKey::from_secret(b"fake"),
        )
        .unwrap();
        assert!(!verify(
            &forged,
            &DecodingKey::from_secret(b"fake"),
            "https://team.cloudflareaccess.com",
            "app"
        ));
        assert!(!verify(
            "broken",
            &DecodingKey::from_secret(b"fake"),
            "https://team.cloudflareaccess.com",
            "app"
        ));
    }
}
