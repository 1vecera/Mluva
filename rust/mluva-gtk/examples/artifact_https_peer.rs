//! Private HTTPS proxy serving public pinned artifacts; no application implementation or outbound sockets.
use native_tls::{Identity, TlsAcceptor};
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Read, Write},
    net::{TcpListener, TcpStream},
    path::{Path, PathBuf},
    sync::{Arc, Mutex},
    thread,
    time::{Duration, Instant},
};

fn read_json(path: &Path) -> Value {
    serde_json::from_slice(&fs::read(path).unwrap()).unwrap()
}
fn write_json(path: &Path, value: &Value) {
    fs::write(path, serde_json::to_vec_pretty(value).unwrap()).unwrap();
}
fn headers(stream: &mut impl Read) -> io::Result<String> {
    let mut bytes = vec![];
    while !bytes.ends_with(b"\r\n\r\n") {
        if bytes.len() >= 8192 {
            return Err(io::Error::other("bounded artifact headers exceeded"));
        }
        let mut byte = [0];
        stream.read_exact(&mut byte)?;
        bytes.push(byte[0]);
    }
    String::from_utf8(bytes).map_err(io::Error::other)
}
fn gate(path: Option<&str>, root: &Path) -> io::Result<()> {
    let Some(path) = path else { return Ok(()) };
    let deadline = Instant::now() + Duration::from_secs(90);
    while !Path::new(path).exists() {
        if root.join("stop").exists() || Instant::now() >= deadline {
            return Err(io::Error::other("artifact response gate stopped"));
        }
        thread::sleep(Duration::from_millis(5));
    }
    Ok(())
}
fn connection(mut socket: TcpStream, tls: TlsAcceptor, root: PathBuf, next: Arc<Mutex<usize>>) {
    socket
        .set_read_timeout(Some(Duration::from_secs(15)))
        .unwrap();
    socket
        .set_write_timeout(Some(Duration::from_secs(30)))
        .unwrap();
    let proxy = headers(&mut socket).unwrap();
    let mut connect = proxy.lines().next().unwrap().split_whitespace();
    assert_eq!(connect.next(), Some("CONNECT"));
    let authority = connect.next().unwrap();
    assert!(matches!(authority, "huggingface.co:443" | "github.com:443"));
    assert!(matches!(connect.next(), Some("HTTP/1.0" | "HTTP/1.1")));
    assert!(!proxy.to_ascii_lowercase().contains("authorization:"));
    socket
        .write_all(b"HTTP/1.1 200 Connection established\r\n\r\n")
        .unwrap();
    let mut stream = tls.accept(socket).unwrap();
    let request = headers(&mut stream).unwrap();
    let mut parts = request.lines().next().unwrap().split_whitespace();
    assert_eq!(parts.next(), Some("GET"));
    let uri = parts.next().unwrap();
    assert!(matches!(parts.next(), Some("HTTP/1.0" | "HTTP/1.1")));
    assert!(!request.to_ascii_lowercase().contains("authorization:"));
    let (index, response) = {
        let mut next = next.lock().unwrap();
        let index = *next;
        *next += 1;
        let spec = read_json(&root.join("spec.json"));
        (index, spec["responses"][index].clone())
    };
    assert_eq!(uri, response["uri"].as_str().unwrap());
    assert_eq!(
        authority.trim_end_matches(":443"),
        response["host"].as_str().unwrap()
    );
    let file = Path::new(response["file"].as_str().unwrap());
    let length = fs::metadata(file).unwrap().len();
    let mut receipt = json!({"host":response["host"],"uri":uri,"bytes":0,"complete":false});
    write_json(&root.join(format!("request-{index}.json")), &receipt);
    let mut sent = 0u64;
    let result = (|| -> io::Result<()> {
        gate(response["release"].as_str(), &root)?;
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Length: {length}\r\nContent-Type: application/octet-stream\r\nConnection: close\r\n\r\n"
        )?;
        let mut file = fs::File::open(file)?;
        let mut block = [0u8; 65_536];
        loop {
            let count = file.read(&mut block)?;
            if count == 0 {
                break;
            }
            stream.write_all(&block[..count])?;
            sent += count as u64;
            if response["pause_after"].as_u64() == Some(sent) {
                stream.flush()?;
                write_json(
                    &root.join(format!("paused-{index}.json")),
                    &json!({"bytes":sent}),
                );
                gate(response["resume"].as_str(), &root)?;
            }
        }
        stream.flush()?;
        Ok(())
    })();
    receipt["bytes"] = json!(sent);
    receipt["complete"] = json!(result.is_ok());
    write_json(&root.join(format!("response-{index}.json")), &receipt);
    result.expect("artifact transfer");
    assert_eq!(sent, length);
}
fn main() {
    let private = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap());
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    let root = PathBuf::from(std::env::args_os().nth(1).unwrap());
    assert!(root.starts_with(&private));
    let identity = Identity::from_pkcs8(
        &fs::read(root.join("cert.pem")).unwrap(),
        &fs::read(root.join("key.pem")).unwrap(),
    )
    .unwrap();
    let tls = TlsAcceptor::new(identity).unwrap();
    let socket = TcpListener::bind("127.0.0.1:48118").unwrap();
    socket.set_nonblocking(true).unwrap();
    write_json(&root.join("ready.json"), &json!({"pid":std::process::id()}));
    let next = Arc::new(Mutex::new(0usize));
    let mut children = vec![];
    while !root.join("stop").exists() {
        match socket.accept() {
            Ok((stream, _)) => {
                let (tls, root, next) = (tls.clone(), root.clone(), next.clone());
                children.push(thread::spawn(move || connection(stream, tls, root, next)));
            }
            Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                thread::sleep(Duration::from_millis(5));
            }
            Err(error) => panic!("private artifact socket: {error}"),
        }
    }
    for child in children {
        child.join().unwrap();
    }
}
