//! Independent synthetic phone client; never uses a microphone or live session.
use std::io::{Read, Write};
fn main() {
    let mut bytes = Vec::new();
    std::io::stdin().read_to_end(&mut bytes).unwrap();
    assert!(bytes.len() <= mluva_core::phone::MAX_MESSAGE_BYTES);
    let mut stream =
        std::os::unix::net::UnixStream::connect(mluva_core::phone::socket_path().unwrap()).unwrap();
    stream
        .set_read_timeout(Some(std::time::Duration::from_secs(10)))
        .unwrap();
    stream
        .write_all(&(bytes.len() as u32).to_be_bytes())
        .unwrap();
    stream.write_all(&bytes).unwrap();
    let mut count = [0; 4];
    stream.read_exact(&mut count).unwrap();
    let mut reply = vec![0; u32::from_be_bytes(count) as usize];
    stream.read_exact(&mut reply).unwrap();
    std::io::stdout().write_all(&reply).unwrap();
}
