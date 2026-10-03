use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{collections::BTreeMap, fs, path::Path};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub fn sha256(bytes: impl AsRef<[u8]>) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

pub fn read_json(path: &Path) -> Result<Value> {
    // Parser diagnostics may contain user-controlled text. Never print it.
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|_| "A widget JSON document is invalid.".into())
}

fn ascii_json(text: &str) -> String {
    let mut escaped = String::new();
    for character in text.chars() {
        if character >= '\u{7f}' {
            for unit in character.encode_utf16(&mut [0; 2]) {
                escaped.push_str(&format!("\\u{unit:04x}"));
            }
        } else {
            escaped.push(character);
        }
    }
    escaped
}

pub fn pretty_json(value: &Value) -> Result<String> {
    Ok(ascii_json(&serde_json::to_string_pretty(value)?) + "\n")
}

pub fn hash_list_json(hashes: &BTreeMap<String, String>) -> Result<String> {
    let entries = hashes
        .iter()
        .map(|(key, value)| {
            Ok(format!(
                "{}: {}",
                ascii_json(&serde_json::to_string(key)?),
                serde_json::to_string(value)?
            ))
        })
        .collect::<Result<Vec<_>>>()?;
    Ok(format!("{{{}}}", entries.join(", ")))
}

pub fn file_hashes(directory: &Path) -> Result<BTreeMap<String, String>> {
    fn visit(root: &Path, path: &Path, result: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let kind = entry.file_type()?;
            if kind.is_symlink() {
                return Err(format!("Preserved symbolic link: {}", path.display()).into());
            }
            if kind.is_dir() {
                visit(root, &path, result)?;
            } else if kind.is_file() && path != root.join(super::RECEIPT) {
                result.insert(
                    path.strip_prefix(root)?
                        .to_str()
                        .ok_or("The widget filename is invalid.")?
                        .to_owned(),
                    sha256(fs::read(&path)?),
                );
            }
        }
        Ok(())
    }
    let mut result = BTreeMap::new();
    visit(directory, directory, &mut result)?;
    Ok(result)
}

pub fn copy_tree(source: &Path, target: &Path) -> Result<()> {
    if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::create_dir(target)?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy_tree(&entry.path(), &destination)?;
        } else if kind.is_file() {
            fs::copy(entry.path(), destination)?;
        } else {
            return Err(format!("Preserved symbolic link: {}", entry.path().display()).into());
        }
    }
    fs::set_permissions(target, fs::metadata(source)?.permissions())?;
    Ok(())
}
