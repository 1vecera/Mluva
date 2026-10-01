use crate::{ProviderError, Result};
use serde_json::Value;
use std::path::Path;

pub fn encode(fields: &[(&str, &str)], path: &Path, boundary: &str) -> Result<Vec<u8>> {
    let mut body = Vec::new();
    for (name, value) in fields {
        body.extend_from_slice(
            format!(
                "--{boundary}\r\nContent-Disposition: form-data; name=\"{name}\"\r\n\r\n{value}\r\n"
            )
            .as_bytes(),
        );
    }
    let filename = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| ProviderError::message("The audio filename could not be encoded."))?;
    body.extend_from_slice(format!("--{boundary}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\nContent-Type: audio/wav\r\n\r\n").as_bytes());
    body.extend_from_slice(&std::fs::read(path).map_err(|error| ProviderError(error.to_string()))?);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    Ok(body)
}

/// Keep the released request spacing and ASCII escaping without an interpreter.
pub(crate) fn json_body(value: &Value) -> Vec<u8> {
    let plain = mluva_core::json::spaced(value);
    let mut encoded = String::with_capacity(plain.len());
    for character in plain.chars() {
        if character < '\u{7f}' {
            encoded.push(character);
        } else {
            for unit in character.encode_utf16(&mut [0; 2]) {
                encoded.push_str(&format!("\\u{unit:04x}"));
            }
        }
    }
    encoded.into_bytes()
}
