//! Real bounded loopback HTTP peer shared by recording and desktop acceptance tests.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use serde_json::{Value, json};
use std::{
    collections::VecDeque,
    io::{Read, Write},
    net::TcpListener,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, Ordering},
    },
    thread,
    time::Duration,
};
#[path = "../../../mluva-providers/tests/support/codex_response.rs"]
#[allow(dead_code)]
pub mod codex_response;
pub struct Peer {
    pub address: String,
    remaining: Arc<Mutex<VecDeque<Value>>>,
    pub observed: Arc<Mutex<Vec<Value>>>,
    failures: Arc<Mutex<Vec<String>>>,
    stop: Arc<AtomicBool>,
    thread: Option<thread::JoinHandle<()>>,
}
impl Peer {
    pub fn new(responses: &[Value]) -> Self {
        Self::bind("127.0.0.1:0", responses)
    }
    pub fn bind(address: &str, responses: &[Value]) -> Self {
        let listener = TcpListener::bind(address).unwrap();
        listener.set_nonblocking(true).unwrap();
        let address = format!("http://{}", listener.local_addr().unwrap());
        let remaining = Arc::new(Mutex::new(
            responses.iter().cloned().collect::<VecDeque<_>>(),
        ));
        let observed = Arc::new(Mutex::new(vec![]));
        let failures = Arc::new(Mutex::new(vec![]));
        let stop = Arc::new(AtomicBool::new(false));
        let (pending, wire, errors, stopping) = (
            remaining.clone(),
            observed.clone(),
            failures.clone(),
            stop.clone(),
        );
        let handle = thread::spawn(move || {
            while !stopping.load(Ordering::Acquire) {
                match listener.accept() {
                    Ok((stream, _)) => {
                        stream
                            .set_read_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        stream
                            .set_write_timeout(Some(Duration::from_secs(3)))
                            .unwrap();
                        if let Err(error) = exchange(stream, &pending, &wire) {
                            errors.lock().unwrap().push(error.to_string());
                        }
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                        thread::sleep(Duration::from_millis(2))
                    }
                    Err(error) => {
                        errors.lock().unwrap().push(error.to_string());
                        break;
                    }
                }
            }
        });
        Self {
            address,
            remaining,
            observed,
            failures,
            stop,
            thread: Some(handle),
        }
    }
    pub fn finish(&mut self) -> Vec<Value> {
        self.stop.store(true, Ordering::Release);
        self.thread.take().unwrap().join().unwrap();
        assert!(
            self.remaining.lock().unwrap().is_empty(),
            "provider requests were skipped"
        );
        assert!(
            self.failures.lock().unwrap().is_empty(),
            "{:?}",
            self.failures.lock().unwrap()
        );
        self.observed.lock().unwrap().clone()
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Release);
        if let Some(handle) = self.thread.take() {
            handle.join().unwrap();
        }
    }
}

pub fn exchange(
    mut stream: impl Read + Write,
    pending: &Mutex<VecDeque<Value>>,
    wire: &Mutex<Vec<Value>>,
) -> Result<(), Box<dyn std::error::Error>> {
    let mut bytes = vec![];
    let header_end = loop {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer)?;
        if count == 0 || bytes.len() > 1_000_000 {
            return Err("incomplete or oversized request".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
        if let Some(end) = bytes.windows(4).position(|part| part == b"\r\n\r\n") {
            break end + 4;
        }
    };
    let header = std::str::from_utf8(&bytes[..header_end])?.to_owned();
    let mut request = header
        .lines()
        .next()
        .ok_or("missing request")?
        .split_whitespace();
    let method = request.next().ok_or("missing method")?;
    let path = request.next().ok_or("missing request path")?;
    let catalog = method == "GET" && path.ends_with("/models");
    let length: usize = header
        .lines()
        .find_map(|line| {
            line.to_ascii_lowercase()
                .strip_prefix("content-length:")
                .map(|value| value.trim().parse())
        })
        .or_else(|| catalog.then_some(Ok(0)))
        .ok_or("missing content length")??;
    if length > 1_000_000 {
        return Err("oversized request".into());
    }
    while bytes.len() < header_end + length {
        let mut buffer = [0; 4096];
        let count = stream.read(&mut buffer)?;
        if count == 0 {
            return Err("interrupted body".into());
        }
        bytes.extend_from_slice(&buffer[..count]);
    }
    let body = &bytes[header_end..header_end + length];
    let response = pending
        .lock()
        .unwrap()
        .pop_front()
        .ok_or("unexpected provider request")?;
    let codex = path == "/responses";
    let speech = matches!(path, "/speech-to-text" | "/v1/speech-to-text")
        || path.ends_with("/audio/transcriptions");
    if let Some(expected) = response["expected_path"].as_str()
        && path != expected
    {
        return Err("unexpected fixed provider endpoint".into());
    }
    if let Some(expected) = response["expected_headers"].as_object() {
        for (name, value) in expected {
            let values: Vec<_> = header
                .lines()
                .filter_map(|line| {
                    let (key, value) = line.split_once(':')?;
                    key.eq_ignore_ascii_case(name).then_some(value.trim())
                })
                .collect();
            if values != [value.as_str().ok_or("invalid expected header")?] {
                return Err("provider authentication header differs".into());
            }
        }
    }
    let validate_route = || -> Result<(), Box<dyn std::error::Error>> {
        if speech != (response["route"] == "speech")
            || catalog != (response["route"] == "catalog")
            || codex != (response["route"] == "codex")
            || (!catalog && method != "POST")
        {
            return Err("provider request order differs".into());
        }
        Ok(())
    };
    if codex {
        validate_route()?;
        let value = json!({"path":path,"json":serde_json::from_slice::<Value>(body)?});
        let path = response["request_log"]
            .as_str()
            .ok_or("missing SDK request log")?;
        std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)?
            .write_all(format!("{value}\n").as_bytes())?;
        wire.lock().unwrap().push(value);
    }
    // An independent endpoint can hold a request before observing its body.
    // This exposes ordering against actual picker/editor processes, without
    // changing an application callback or its clocks.
    if let Some(gate) = response["wait_for_file"].as_str() {
        std::fs::write(
            response["incoming_file"]
                .as_str()
                .ok_or("missing gate receipt")?,
            b"arrived",
        )?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !std::path::Path::new(gate).exists() {
            if std::time::Instant::now() >= deadline {
                return Err("provider gate expired".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    if !codex {
        validate_route()?;
    }
    if catalog {
        wire.lock().unwrap().push(json!({"path":path,"authorization":header.lines().any(|line|line.to_ascii_lowercase().starts_with("authorization:"))}));
    } else if speech {
        wire.lock()
            .unwrap()
            .push(json!({"path": path, "fields": multipart_fields(body)?}));
    } else if !codex {
        if path != "/chat/completions" {
            return Err("unexpected rewrite endpoint".into());
        }
        wire.lock()
            .unwrap()
            .push(json!({"path": path, "json": serde_json::from_slice::<Value>(body)?}));
    }
    // Preserve the earlier pre-observation gate. This separate gate records
    // the actual parsed body before holding the ordinary HTTPS response.
    if let Some(gate) = response["wait_after_body"].as_str() {
        let receipt = std::path::Path::new(
            response["body_receipt"]
                .as_str()
                .ok_or("missing body receipt")?,
        );
        let pending_receipt = receipt.with_extension("part");
        std::fs::write(
            &pending_receipt,
            serde_json::to_vec(wire.lock().unwrap().last().ok_or("missing observed body")?)?,
        )?;
        std::fs::rename(pending_receipt, receipt)?;
        let deadline = std::time::Instant::now() + Duration::from_secs(8);
        while !std::path::Path::new(gate).exists() {
            if std::time::Instant::now() >= deadline {
                return Err("observed provider gate expired".into());
            }
            thread::sleep(Duration::from_millis(2));
        }
    }
    let body = if codex {
        codex_response::encode(response["events"].as_array().ok_or("missing SDK events")?)
    } else if speech || catalog {
        serde_json::to_vec(&response["payload"])?
    } else if response["malformed"] == true {
        b"data: {broken}\n\n".to_vec()
    } else {
        format!("data: {}\n\ndata: [DONE]\n\n", json!({"choices": [{"index":0,"delta":{"content":response["text"].as_str().unwrap_or("")},"finish_reason":"stop"}]})).into_bytes()
    };
    if let Some(delay) = response["delay_ms"].as_u64() {
        thread::sleep(Duration::from_millis(delay));
    }
    let sent = (|| -> std::io::Result<()> {
        write!(
            stream,
            "HTTP/1.1 {} Fixture\r\nContent-Length: {}\r\nContent-Type: {}\r\nConnection: close\r\n\r\n",
            response["status"].as_u64().unwrap(),
            body.len(),
            if speech || catalog {
                "application/json"
            } else {
                "text/event-stream"
            }
        )?;
        stream.write_all(&body)
    })();
    if let Err(error) = sent
        && (response["allow_disconnect"] != true
            || !matches!(
                error.kind(),
                std::io::ErrorKind::BrokenPipe | std::io::ErrorKind::ConnectionReset
            ))
    {
        return Err(error.into());
    }
    if let Some(path) = response["completed_file"].as_str() {
        std::fs::write(path, b"response finished")?;
    }
    Ok(())
}

fn multipart_fields(body: &[u8]) -> Result<Value, Box<dyn std::error::Error>> {
    let line_end = body
        .windows(2)
        .position(|bytes| bytes == b"\r\n")
        .ok_or("missing multipart boundary")?;
    let marker = &body[..line_end];
    let mut fields = serde_json::Map::new();
    let mut position = line_end + 2;
    loop {
        let end = body[position..]
            .windows(4)
            .position(|bytes| bytes == b"\r\n\r\n")
            .ok_or("missing part header")?
            + position;
        let header = std::str::from_utf8(&body[position..end])?;
        let name = header
            .split("name=\"")
            .nth(1)
            .and_then(|value| value.split('"').next())
            .ok_or("missing part name")?;
        let content_start = end + 4;
        let mut boundary = b"\r\n".to_vec();
        boundary.extend_from_slice(marker);
        let content_end = body[content_start..]
            .windows(boundary.len())
            .position(|bytes| bytes == boundary)
            .ok_or("missing part terminator")?
            + content_start;
        let content = &body[content_start..content_end];
        fields.insert(
            name.into(),
            if header.contains("filename=\"") {
                json!(STANDARD.encode(content))
            } else {
                json!(std::str::from_utf8(content)?)
            },
        );
        position = content_end + boundary.len();
        if body[position..].starts_with(b"--") {
            break;
        }
        if !body[position..].starts_with(b"\r\n") {
            return Err("malformed multipart separator".into());
        }
        position += 2;
    }
    Ok(Value::Object(fields))
}
