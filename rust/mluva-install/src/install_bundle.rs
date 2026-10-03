//! Validate a copied native release before executing or publishing any of it.
use crate::install_paths::{Result, regular};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    fs,
    io::Read,
    os::unix::fs::{OpenOptionsExt, PermissionsExt, symlink},
    path::{Path, PathBuf},
};

pub fn copy(source: &Path, target: &Path) -> Result<()> {
    if !source.symlink_metadata()?.is_dir() {
        return Err("The bundle must be a real directory.".into());
    }
    fs::create_dir(target)?;
    fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
    for entry in fs::read_dir(source)? {
        let entry = entry?;
        let destination = target.join(entry.file_name());
        let kind = entry.file_type()?;
        if kind.is_dir() {
            copy(&entry.path(), &destination)?;
        } else if kind.is_symlink() {
            symlink(fs::read_link(entry.path())?, destination)?;
        } else if kind.is_file() {
            let mut input = fs::OpenOptions::new()
                .read(true)
                .custom_flags(libc::O_NOFOLLOW)
                .open(entry.path())?;
            let mut output = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .open(&destination)?;
            std::io::copy(&mut input, &mut output)?;
            output.set_permissions(fs::Permissions::from_mode(
                input.metadata()?.permissions().mode() & 0o777,
            ))?;
        } else {
            return Err("The bundle contains an unexpected file type.".into());
        }
    }
    Ok(())
}

pub fn validate(root: &Path) -> Result<()> {
    fn inventory(
        root: &Path,
        path: &Path,
        files: &mut BTreeMap<String, String>,
        links: &mut BTreeMap<String, PathBuf>,
    ) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            let relative = path
                .strip_prefix(root)?
                .to_str()
                .ok_or("Invalid bundle filename")?
                .to_owned();
            let meta = path.symlink_metadata()?;
            if meta.is_dir() {
                if meta.permissions().mode() & 0o777 != 0o755 {
                    return Err("Invalid bundle directory permissions.".into());
                }
                inventory(root, &path, files, links)?;
            } else if meta.is_symlink() {
                links.insert(relative, fs::read_link(path)?);
            } else if meta.is_file() {
                let mode = if relative.starts_with("bin/") || relative.ends_with(".sh") {
                    0o755
                } else {
                    0o644
                };
                if meta.permissions().mode() & 0o7777 != mode {
                    return Err("Invalid bundle file permissions.".into());
                }
                if relative == ".mluva-native.json" {
                    continue;
                }
                if relative.ends_with(".py")
                    || relative.ends_with(".pyc")
                    || relative.contains(".venv")
                {
                    return Err("The native bundle contains a Python runtime file.".into());
                }
                let mut file = fs::File::open(path)?;
                let mut hash = Sha256::new();
                let mut bytes = [0; 65536];
                loop {
                    let length = file.read(&mut bytes)?;
                    if length == 0 {
                        break;
                    }
                    hash.update(&bytes[..length]);
                }
                files.insert(
                    relative,
                    hash.finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                );
            } else {
                return Err("The native bundle contains an unexpected file type.".into());
            }
        }
        Ok(())
    }
    let receipt = root.join(".mluva-native.json");
    if !regular(&receipt) {
        return Err("The native bundle has no regular release inventory.".into());
    }
    let receipt: Value = serde_json::from_slice(&fs::read(receipt)?)
        .map_err(|_| "The native release inventory is invalid.")?;
    if receipt["schema"] != 1
        || receipt["application"] != "com.mluva.Linux"
        || receipt["implementation"] != "rust"
        || receipt["version"] != env!("CARGO_PKG_VERSION")
    {
        return Err("The native release inventory does not match this installer.".into());
    }
    let (mut files, mut links) = (BTreeMap::new(), BTreeMap::new());
    inventory(root, root, &mut files, &mut links)?;
    let expected_links = json!({"mluva-shell":"bin/mluva-shell","mluva-narrate":"bin/mluva-narrate","mluva-screenshot-editor":"bin/mluva-screenshot-editor","uninstall.sh":"bin/mluva-uninstall","install.sh":"bin/mluva-install"});
    if receipt["sha256"] != json!(files)
        || receipt["links"] != json!(links)
        || receipt["links"] != expected_links
    {
        return Err("The native bundle differs from its release inventory.".into());
    }
    for name in [
        "mluva",
        "mluva-shell",
        "mluva-narrate",
        "mluva-asr-worker",
        "mluva-audio-cleanup",
        "mluva-install-widget",
        "mluva-screenshot-editor",
        "mluva-uninstall",
        "mluva-install",
    ] {
        let path = root.join("bin").join(name);
        if !regular(&path) {
            return Err("The native bundle is missing a production executable.".into());
        }
        let mut header = [0; 20];
        fs::File::open(path)?.read_exact(&mut header)?;
        let architecture = if cfg!(target_arch = "x86_64") {
            62
        } else if cfg!(target_arch = "aarch64") {
            183
        } else {
            0
        };
        if &header[..4] != b"\x7fELF"
            || header[4] != 2
            || header[5] != 1
            || !matches!(header[16], 2 | 3)
            || header[17] != 0
            || architecture == 0
            || u16::from_le_bytes([header[18], header[19]]) != architecture
        {
            return Err("A bundle executable does not match the native Linux architecture.".into());
        }
    }
    Ok(())
}
