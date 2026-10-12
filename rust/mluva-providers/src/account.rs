//! Optional account transport. Only typed text documents enter it; native capture/providers are independent.
use base64::{Engine, engine::general_purpose::URL_SAFE_NO_PAD};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    process::Stdio,
    sync::Arc,
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::{io::AsyncWriteExt, sync::Mutex};
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::client::IntoClientRequest};
use url::Url;

pub const ACCOUNT_ORIGIN: &str = "https://mluva.danielvecera.com";
pub const CALLBACK: &str = "mluva://account";
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountConfig {
    pub api: String,
    pub websocket: String,
    pub login: String,
    pub client_id: String,
    pub origin: String,
}
#[derive(Clone, Deserialize, Serialize)]
pub struct AccountAuth {
    pub access_token: String,
    pub refresh_token: String,
    pub expires: u64,
    pub owner: String,
    pub device_id: String,
}
#[derive(Debug, thiserror::Error)]
#[error("{message}")]
pub struct AccountError {
    pub status: u16,
    pub message: String,
}
pub type Result<T> = std::result::Result<T, AccountError>;
fn error(status: u16, message: &str) -> AccountError {
    AccountError {
        status,
        message: message.into(),
    }
}
fn network(_: impl std::fmt::Display) -> AccountError {
    error(
        503,
        "Account connection failed. Local recording still works; retry sync when online.",
    )
}
fn epoch() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
fn client() -> Result<reqwest::Client> {
    reqwest::Client::builder()
        .redirect(reqwest::redirect::Policy::none())
        .timeout(Duration::from_secs(15))
        .build()
        .map_err(network)
}
fn endpoint(value: &str, websocket: bool) -> Result<Url> {
    let url = Url::parse(value).map_err(network)?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost"));
    let secure = if websocket {
        url.scheme() == "wss"
    } else {
        url.scheme() == "https"
    };
    if !(secure || local && matches!(url.scheme(), "http" | "ws"))
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(error(400, "Account endpoints must use HTTPS."));
    }
    Ok(url)
}
impl AccountConfig {
    pub fn validate(&self, origin: &str) -> Result<()> {
        endpoint(&self.api, false)?;
        endpoint(&self.websocket, true)?;
        endpoint(&self.login, false)?;
        if self.origin.trim_end_matches('/') != origin.trim_end_matches('/') {
            return Err(error(
                400,
                "Account host configuration does not match its address.",
            ));
        }
        Ok(())
    }
    pub async fn load(origin: &str) -> Result<Self> {
        let origin = endpoint(origin, false)?;
        let response = client()?
            .get(origin.join("/config.json").map_err(network)?)
            .send()
            .await
            .map_err(network)?;
        let body = response.bytes().await.map_err(network)?;
        if body.len() > 16000 {
            return Err(error(400, "Invalid account configuration."));
        }
        let config: Self = serde_json::from_slice(&body).map_err(network)?;
        config.validate(origin.as_str())?;
        Ok(config)
    }
}
pub struct AccountLogin {
    pub url: String,
    verifier: String,
    state: String,
    config: AccountConfig,
    started: std::time::Instant,
}
impl AccountLogin {
    pub fn new(config: AccountConfig) -> Result<Self> {
        let random = || -> Result<String> {
            let mut b = [0u8; 32];
            getrandom::fill(&mut b).map_err(network)?;
            Ok(URL_SAFE_NO_PAD.encode(b))
        };
        let verifier = random()?;
        let state = random()?;
        let challenge = URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()));
        let mut url = endpoint(&config.login, false)?
            .join("/oauth2/authorize")
            .map_err(network)?;
        url.query_pairs_mut().extend_pairs([
            ("response_type", "code"),
            ("client_id", &config.client_id),
            ("redirect_uri", CALLBACK),
            ("scope", "openid email"),
            ("state", &state),
            ("code_challenge", &challenge),
            ("code_challenge_method", "S256"),
        ]);
        Ok(Self {
            url: url.into(),
            verifier,
            state,
            config,
            started: std::time::Instant::now(),
        })
    }
    pub async fn finish(self, callback: &str) -> Result<AccountClient> {
        if self.started.elapsed() > Duration::from_secs(600) {
            return Err(error(401, "Sign-in expired. Start again from Mluva."));
        }
        let url = Url::parse(callback).map_err(network)?;
        if url.scheme() != "mluva" || url.host_str() != Some("account") || !url.path().is_empty() {
            return Err(error(400, "Invalid sign-in callback."));
        }
        let pairs: std::collections::HashMap<_, _> = url.query_pairs().collect();
        if pairs.get("state").map(|v| v.as_ref()) != Some(&self.state) {
            return Err(error(400, "Sign-in could not be verified. Try again."));
        }
        let code = pairs
            .get("code")
            .ok_or_else(|| error(400, "Sign-in was cancelled."))?;
        let response = client()?
            .post(format!("{}/oauth2/token", self.config.login))
            .form(&[
                ("grant_type", "authorization_code"),
                ("client_id", &self.config.client_id),
                ("redirect_uri", CALLBACK),
                ("code", code),
                ("code_verifier", &self.verifier),
            ])
            .send()
            .await
            .map_err(network)?;
        let tokens = decode(response).await?;
        let access = tokens["access_token"]
            .as_str()
            .ok_or_else(|| error(401, "Sign-in returned no access token."))?
            .to_owned();
        let claims: Value = serde_json::from_slice(
            &URL_SAFE_NO_PAD
                .decode(access.split('.').nth(1).unwrap_or(""))
                .map_err(network)?,
        )
        .map_err(network)?;
        // Identity here selects local cache only. The API validates the JWT before every data operation.
        let owner = claims["sub"]
            .as_str()
            .filter(|v| !v.is_empty())
            .ok_or_else(|| error(401, "Sign-in returned no account identity."))?
            .to_owned();
        let auth = AccountAuth {
            access_token: access,
            refresh_token: tokens["refresh_token"].as_str().unwrap_or("").into(),
            expires: epoch() + tokens["expires_in"].as_u64().unwrap_or(0),
            owner,
            device_id: uuid::Uuid::new_v4().to_string(),
        };
        Ok(AccountClient::new(self.config, auth))
    }
}
#[derive(Clone)]
pub struct AccountClient {
    pub config: AccountConfig,
    auth: Arc<Mutex<AccountAuth>>,
}
async fn decode(response: reqwest::Response) -> Result<Value> {
    let status = response.status().as_u16();
    let bytes = response.bytes().await.map_err(network)?;
    if bytes.len() > 8_000_000 {
        return Err(error(413, "Account response is too large."));
    }
    let value: Value = serde_json::from_slice(&bytes).map_err(network)?;
    if status >= 400 {
        return Err(error(
            status,
            match status {
                401 => "Sign in again to resume account sync.",
                403 => "This device was removed. Sign out before connecting again.",
                409 => "This note changed on another device. Review both versions.",
                410 => "This note was deleted on another device.",
                400 | 413 => {
                    "A conversation cannot fit the account document limits. Keep it locally, shorten its working text or export it. Other notes can still sync."
                }
                _ => "Account request failed. Your local work is safe; retry when online.",
            },
        ));
    }
    Ok(value)
}
impl AccountClient {
    pub fn new(config: AccountConfig, auth: AccountAuth) -> Self {
        Self {
            config,
            auth: Arc::new(Mutex::new(auth)),
        }
    }
    pub async fn identity(&self) -> (String, String) {
        let a = self.auth.lock().await;
        (a.owner.clone(), a.device_id.clone())
    }
    async fn token(&self) -> Result<String> {
        let mut auth = self.auth.lock().await;
        if auth.access_token.is_empty() {
            return Err(error(401, "Signed out. Sign in to sync again."));
        }
        if auth.expires < epoch() + 60 {
            let response = client()?
                .post(format!("{}/oauth2/token", self.config.login))
                .form(&[
                    ("grant_type", "refresh_token"),
                    ("client_id", &self.config.client_id),
                    ("refresh_token", &auth.refresh_token),
                ])
                .send()
                .await
                .map_err(network)?;
            let tokens = decode(response).await?;
            auth.access_token = tokens["access_token"]
                .as_str()
                .ok_or_else(|| error(401, "Sign in again."))?
                .into();
            auth.expires = epoch() + tokens["expires_in"].as_u64().unwrap_or(0);
            if let Some(refresh) = tokens["refresh_token"].as_str() {
                auth.refresh_token = refresh.into();
            }
        }
        Ok(auth.access_token.clone())
    }
    pub async fn api(&self, method: &str, path: &str, mut body: Value) -> Result<Value> {
        let token = self.token().await?;
        let (_, id) = self.identity().await;
        let mut url = endpoint(&format!("{}{}", self.config.api, path), false)?;
        url.query_pairs_mut().append_pair("deviceId", &id);
        let mut request = client()?
            .request(
                reqwest::Method::from_bytes(method.as_bytes()).map_err(network)?,
                url,
            )
            .bearer_auth(token)
            .header("origin", &self.config.origin);
        if !body.is_null() {
            body["deviceId"] = json!(id);
            request = request
                .header("content-type", "application/json")
                .body(serde_json::to_vec(&body).map_err(network)?);
        }
        decode(request.send().await.map_err(network)?).await
    }
    pub async fn connect(&self) -> Result<WebSocketStream<MaybeTlsStream<tokio::net::TcpStream>>> {
        let ticket = self.api("POST", "/ticket", json!({})).await?;
        let mut url = endpoint(&self.config.websocket, true)?;
        url.query_pairs_mut().append_pair(
            "ticket",
            ticket["ticket"]
                .as_str()
                .ok_or_else(|| error(401, "Missing connection ticket."))?,
        );
        let mut request = url.as_str().into_client_request().map_err(network)?;
        request
            .headers_mut()
            .insert("origin", self.config.origin.parse().map_err(network)?);
        tokio_tungstenite::connect_async(request)
            .await
            .map(|r| r.0)
            .map_err(network)
    }
    /// Store credentials exclusively in the desktop Secret Service, over stdin, never argv or files.
    pub async fn remember(&self) -> Result<()> {
        let data = serde_json::to_vec(&*self.auth.lock().await).map_err(network)?;
        let mut process = tokio::process::Command::new("secret-tool")
            .args([
                "store",
                "--label=Mluva account",
                "application",
                "mluva",
                "origin",
                &self.config.origin,
            ])
            .stdin(Stdio::piped())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .spawn()
            .map_err(|_| {
                error(
                    503,
                    "Keyring unavailable. This sign-in lasts until Mluva closes.",
                )
            })?;
        process
            .stdin
            .take()
            .unwrap()
            .write_all(&data)
            .await
            .map_err(network)?;
        if !process.wait().await.map_err(network)?.success() {
            return Err(error(
                503,
                "Keyring locked. This sign-in lasts until Mluva closes.",
            ));
        }
        Ok(())
    }
    pub async fn restore(config: AccountConfig) -> Result<Option<Self>> {
        let result = tokio::process::Command::new("secret-tool")
            .args(["lookup", "application", "mluva", "origin", &config.origin])
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .output()
            .await
            .map_err(network)?;
        if !result.status.success() || result.stdout.is_empty() {
            return Ok(None);
        }
        let auth = serde_json::from_slice(&result.stdout).map_err(network)?;
        Ok(Some(Self::new(config, auth)))
    }
    pub async fn sign_out(&self) -> Result<()> {
        let auth = {
            let mut current = self.auth.lock().await;
            let auth = current.clone();
            current.access_token.clear();
            current.refresh_token.clear();
            auth
        };
        // Forget locally before touching a potentially offline server.
        let _ = tokio::process::Command::new("secret-tool")
            .args([
                "clear",
                "application",
                "mluva",
                "origin",
                &self.config.origin,
            ])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .kill_on_drop(true)
            .status()
            .await;
        let revoked = client()?
            .delete(format!("{}/devices/{}", self.config.api, auth.device_id))
            .bearer_auth(&auth.access_token)
            .header("origin", &self.config.origin)
            .send()
            .await
            .map_err(network);
        let _ = client()?
            .post(format!("{}/oauth2/revoke", self.config.login))
            .form(&[
                ("token", &auth.refresh_token),
                ("client_id", &self.config.client_id),
            ])
            .send()
            .await;
        match revoked {
            Ok(response) if response.status().is_success() || response.status().as_u16() == 403 => {
                Ok(())
            }
            _ => Err(network("revocation")),
        }
    }
}
