#![allow(dead_code)]
use std::collections::HashMap;
use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};
use tokio::net::{TcpListener, TcpStream};
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_tungstenite::tungstenite::handshake::server::{
    Request as UpgradeRequest, Response as UpgradeResponse,
};
use tokio_tungstenite::{WebSocketStream, accept_hdr_async};

pub fn unhex(value: &str) -> Vec<u8> {
    value
        .as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}

#[derive(Debug)]
pub struct Request {
    pub method: String,
    pub path: String,
    pub headers: HashMap<String, String>,
    pub body: Vec<u8>,
}

pub async fn request(socket: &mut (impl AsyncRead + Unpin)) -> Request {
    let mut raw = vec![];
    let end = loop {
        let byte = socket.read_u8().await.unwrap();
        raw.push(byte);
        assert!(raw.len() < 16_384, "unexpected large request header");
        if raw.ends_with(b"\r\n\r\n") {
            break raw.len();
        }
    };
    let header = std::str::from_utf8(&raw[..end]).unwrap();
    let mut lines = header.split("\r\n");
    let mut first = lines.next().unwrap().split_whitespace();
    let method = first.next().unwrap().into();
    let path = first.next().unwrap().into();
    let headers: HashMap<_, _> = lines
        .filter_map(|line| line.split_once(':'))
        .map(|(key, value)| (key.to_ascii_lowercase(), value.trim().into()))
        .collect();
    let size = headers
        .get("content-length")
        .map_or(0, |value: &String| value.parse::<usize>().unwrap());
    assert!(size < 32_000_000, "unexpected large synthetic request");
    let mut body = vec![0; size];
    socket.read_exact(&mut body).await.unwrap();
    Request {
        method,
        path,
        headers,
        body,
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
    pub fragment: usize,
    pub stall: bool,
}
impl Response {
    pub fn ok(body: impl Into<Vec<u8>>) -> Self {
        Self {
            status: 200,
            headers: vec![],
            body: body.into(),
            fragment: 0,
            stall: false,
        }
    }
}

pub async fn respond(socket: &mut (impl AsyncWrite + Unpin), response: &Response) {
    if response.stall {
        std::future::pending::<()>().await;
    }
    let mut header = format!(
        "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nConnection: close\r\n",
        response.status,
        response.body.len()
    );
    for (key, value) in &response.headers {
        header.push_str(&format!("{key}: {value}\r\n"));
    }
    header.push_str("\r\n");
    if socket.write_all(header.as_bytes()).await.is_err() {
        return;
    }
    for part in response.body.chunks(if response.fragment == 0 {
        response.body.len().max(1)
    } else {
        response.fragment
    }) {
        if socket.write_all(part).await.is_err() {
            return;
        }
        tokio::task::yield_now().await;
    }
    let _ = socket.shutdown().await;
}

pub async fn http_once(response: Response) -> (String, oneshot::Receiver<Request>, JoinHandle<()>) {
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("http://{}", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();
    let server = tokio::spawn(async move {
        let (mut socket, _) = listener.accept().await.unwrap();
        let observed = request(&mut socket).await;
        let _ = tx.send(observed);
        respond(&mut socket, &response).await;
    });
    (url, rx, server)
}

// Tungstenite requires its unboxed HTTP error response in handshake callbacks.
#[allow(clippy::result_large_err)]
pub async fn websocket<F, Fut>(
    action: F,
) -> (
    String,
    oneshot::Receiver<(String, HashMap<String, String>)>,
    JoinHandle<Fut::Output>,
)
where
    F: FnOnce(WebSocketStream<TcpStream>) -> Fut + Send + 'static,
    Fut: std::future::Future + Send + 'static,
    Fut::Output: Send + 'static,
{
    let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let url = format!("ws://{}/realtime", listener.local_addr().unwrap());
    let (tx, rx) = oneshot::channel();
    let task = tokio::spawn(async move {
        let (socket, _) = listener.accept().await.unwrap();
        let connection = accept_hdr_async(
            socket,
            move |request: &UpgradeRequest, response: UpgradeResponse| {
                let headers = request
                    .headers()
                    .iter()
                    .map(|(key, value)| (key.to_string(), value.to_str().unwrap().into()))
                    .collect();
                let _ = tx.send((request.uri().to_string(), headers));
                Ok(response)
            },
        )
        .await
        .unwrap();
        action(connection).await
    });
    (url, rx, task)
}
