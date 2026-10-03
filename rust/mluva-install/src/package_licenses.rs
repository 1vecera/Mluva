//! Preserve notices from checksum-verified registry archives, not a developer's
//! possibly modified unpacked cache. Cargo metadata supplies the resolved graph.
use crate::{Result, copy_file, regular};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::{BTreeMap, BTreeSet},
    env, fs,
    io::{Read, Seek, SeekFrom, Write},
    os::unix::fs::PermissionsExt,
    path::{Component, Path, PathBuf},
    process::{Command, Stdio},
};

const REGISTRY: &str = "registry+https://github.com/rust-lang/crates.io-index";

fn string<'a>(value: &'a Value, field: &str) -> Result<&'a str> {
    value[field]
        .as_str()
        .filter(|text| !text.is_empty())
        .ok_or_else(|| format!("Missing dependency metadata field: {field}").into())
}
fn array(value: &Value) -> Result<&Vec<Value>> {
    value
        .as_array()
        .ok_or_else(|| "Invalid dependency metadata array.".into())
}
fn relative(path: &Path) -> bool {
    !path.as_os_str().is_empty()
        && path
            .components()
            .all(|part| matches!(part, Component::Normal(_)))
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|byte| format!("{byte:02x}")).collect()
}
fn archive(cache: &Path, name: &str, checksum: &str) -> Result<fs::File> {
    for registry in fs::read_dir(cache)? {
        let path = registry?.path().join(format!("{name}.crate"));
        if !path.exists() {
            continue;
        }
        regular(&path)?;
        let mut file = fs::File::open(&path)?;
        let mut hash = Sha256::new();
        let mut block = [0; 65536];
        loop {
            let size = file.read(&mut block)?;
            if size == 0 {
                break;
            }
            hash.update(&block[..size]);
        }
        if hex(&hash.finalize()) == checksum {
            file.seek(SeekFrom::Start(0))?;
            return Ok(file);
        }
    }
    Err(format!("No checksum-verified cached archive for {name}; fetch the locked dependencies before packaging.").into())
}
fn notice_name(name: &std::ffi::OsStr) -> bool {
    let name = name.to_string_lossy().to_ascii_lowercase();
    [
        "license",
        "licence",
        "notice",
        "copying",
        "copyright",
        "unlicense",
    ]
    .iter()
    .any(|prefix| name.starts_with(prefix))
}
fn notices(
    file: fs::File,
    name: &str,
    declared: Option<&str>,
    destination: &Path,
) -> Result<Vec<String>> {
    let mut result = BTreeSet::new();
    let mut archive = tar::Archive::new(flate2::read::GzDecoder::new(file));
    for entry in archive.entries()? {
        let mut entry = entry?;
        let path = entry.path()?.into_owned();
        let path = path
            .strip_prefix(name)
            .map_err(|_| "Unexpected registry archive root.")?;
        if path.as_os_str().is_empty() || entry.header().entry_type().is_dir() {
            continue;
        }
        if !relative(path) {
            return Err("Invalid registry notice path.".into());
        }
        let selected = declared.is_some_and(|file| Path::new(file) == path)
            || path.components().any(|part| notice_name(part.as_os_str()));
        if !selected {
            continue;
        }
        if !entry.header().entry_type().is_file() || entry.size() > 1024 * 1024 {
            return Err(format!("Review the non-regular or oversized notice in {name}.").into());
        }
        if matches!(
            path.extension().and_then(|text| text.to_str()),
            Some("py" | "pyc")
        ) {
            return Err(format!(
                "A notice selection for {name} includes a Python helper; review its distribution."
            )
            .into());
        }
        let filename = path
            .to_str()
            .ok_or("Invalid registry notice filename.")?
            .to_owned();
        if !result.insert(filename) {
            return Err("Duplicate registry notice path.".into());
        }
        let target = destination.join(path);
        fs::create_dir_all(target.parent().ok_or("Missing notice directory.")?)?;
        let mut output = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(target)?;
        std::io::copy(&mut entry, &mut output)?;
        output.set_permissions(fs::Permissions::from_mode(0o644))?;
    }
    if result.is_empty() {
        return Err(format!("No upstream notice files found for {name}.").into());
    }
    Ok(result.into_iter().collect())
}
fn directory_modes(path: &Path) -> Result<()> {
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            directory_modes(&entry.path())?;
        }
    }
    fs::set_permissions(path, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn toolchain_notices(source: &Path, root: &Path, cargo_home: &Path) -> Result<Value> {
    let rustc = env::var_os("RUSTC").unwrap_or_else(|| "rustc".into());
    let query = |arguments: &[&str]| -> Result<String> {
        let output = Command::new(&rustc)
            .current_dir(source)
            .env("CARGO_HOME", cargo_home)
            .args(arguments)
            .stdin(Stdio::null())
            .output()?;
        if !output.status.success() {
            return Err("Cannot locate the pinned Rust standard-library notices.".into());
        }
        Ok(String::from_utf8(output.stdout)?.trim().to_owned())
    };
    let version = query(&["--version"])?;
    let pinned: toml::Value =
        toml::from_str(&fs::read_to_string(source.join("rust-toolchain.toml"))?)?;
    let channel = pinned["toolchain"]["channel"]
        .as_str()
        .ok_or("Missing pinned compiler version.")?;
    if version.split_whitespace().nth(1) != Some(channel) {
        return Err("Package notices require the repository's pinned Rust compiler.".into());
    }
    let docs = Path::new(&query(&["--print", "sysroot"])?).join("share/doc/rust");
    let mut names = vec!["COPYRIGHT-library.html".to_owned()];
    for file in fs::read_dir(docs.join("licenses"))? {
        let file = file?;
        if file.path().extension().and_then(|text| text.to_str()) != Some("txt") {
            return Err("Review the unexpected Rust toolchain notice format.".into());
        }
        names.push(format!(
            "licenses/{}",
            file.file_name()
                .to_str()
                .ok_or("Invalid notice filename.")?
        ));
    }
    names.sort();
    for name in &names {
        copy_file(
            &docs.join(name),
            &root.join("rust-standard-library").join(name),
            0o644,
        )?;
    }
    Ok(json!({"rustc":version,
        "notice_directory":"third-party-licenses/rust-standard-library", "notices":names}))
}

pub(super) fn stage(source: &Path, bundle: &Path) -> Result<()> {
    let target = format!("{}-unknown-linux-gnu", env::consts::ARCH);
    // Preserve the caller's cache location when metadata runs in another source
    // directory. Cargo and the archive reader must resolve a relative home alike.
    let cargo_home = env::var_os("CARGO_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cargo")))
        .ok_or("Cargo's registry cache is unavailable.")?;
    let cargo_home = std::path::absolute(cargo_home)?;
    let cargo = env::var_os("CARGO").unwrap_or_else(|| "cargo".into());
    let output = Command::new(cargo)
        .current_dir(source)
        .env("CARGO_HOME", &cargo_home)
        .args([
            "metadata",
            "--locked",
            "--offline",
            "--format-version",
            "1",
            "--filter-platform",
            &target,
        ])
        .stdin(Stdio::null())
        .output()?;
    if !output.status.success() {
        return Err("Cannot resolve locked Cargo metadata; prepare the pinned toolchain and cached dependencies before packaging.".into());
    }
    let metadata: Value = serde_json::from_slice(&output.stdout)?;
    let members: BTreeSet<_> = array(&metadata["workspace_members"])?
        .iter()
        .map(|id| {
            id.as_str()
                .ok_or_else(|| "Invalid workspace member.".into())
        })
        .collect::<Result<_>>()?;
    let nodes: BTreeMap<_, _> = array(&metadata["resolve"]["nodes"])?
        .iter()
        .map(|node| Ok((string(node, "id")?, node)))
        .collect::<Result<_>>()?;
    let mut pending: Vec<_> = members.iter().copied().collect();
    let mut selected = BTreeSet::new();
    while let Some(id) = pending.pop() {
        if !selected.insert(id) {
            continue;
        }
        let node = nodes.get(id).ok_or("Missing resolved dependency node.")?;
        for dependency in array(&node["deps"])? {
            let mut include = false;
            for kind in array(&dependency["dep_kinds"])? {
                match kind["kind"].as_str() {
                    None if kind["kind"].is_null() => include = true,
                    Some("build") => include = true,
                    Some("dev") => {}
                    _ => return Err("Unrecognized Cargo dependency kind.".into()),
                }
            }
            if include {
                pending.push(string(dependency, "pkg")?);
            }
        }
    }
    let lock: toml::Value = toml::from_str(&fs::read_to_string(source.join("Cargo.lock"))?)?;
    let locked = lock["package"]
        .as_array()
        .ok_or("Invalid dependency lockfile.")?;
    let cache = cargo_home.join("registry/cache");
    let root = bundle.join("third-party-licenses");
    let mut inventory = Vec::new();
    for package in array(&metadata["packages"])? {
        let id = string(package, "id")?;
        if !selected.contains(id) || members.contains(id) {
            continue;
        }
        let name = string(package, "name")?;
        let version = string(package, "version")?;
        let license = string(package, "license")?;
        if package["source"].as_str() != Some(REGISTRY) {
            return Err(format!(
                "Review notice collection for the non-registry dependency {name}."
            )
            .into());
        }
        let identity = format!("{name}-{version}");
        if !relative(Path::new(&identity)) || Path::new(&identity).components().count() != 1 {
            return Err("Invalid dependency identity.".into());
        }
        let checksum = locked
            .iter()
            .find(|item| {
                item["name"].as_str() == Some(name)
                    && item["version"].as_str() == Some(version)
                    && item.get("source").and_then(toml::Value::as_str) == Some(REGISTRY)
            })
            .and_then(|item| item.get("checksum"))
            .and_then(toml::Value::as_str)
            .ok_or("Resolved dependency has no matching locked checksum.")?;
        let files = notices(
            archive(&cache, &identity, checksum)?,
            &identity,
            package["license_file"].as_str(),
            &root.join("crates").join(&identity),
        )?;
        inventory.push(json!({"name":name,"version":version,"license":license,
            "source":REGISTRY,"checksum":checksum,"notice_directory":format!("third-party-licenses/crates/{identity}"),"notices":files}));
    }
    inventory.sort_by(|a, b| {
        (a["name"].as_str(), a["version"].as_str())
            .cmp(&(b["name"].as_str(), b["version"].as_str()))
    });
    copy_file(
        &source.join("rust/mluva-asr/resources/onnx-asr-LICENSE"),
        &root.join("onnx-asr/LICENSE"),
        0o644,
    )?;
    let toolchain = toolchain_notices(source, &root, &cargo_home)?;
    directory_modes(&root)?;
    copy_file(
        &source.join("rust/mluva-install/resources/THIRD_PARTY_NOTICES.md"),
        &bundle.join("THIRD_PARTY_NOTICES.md"),
        0o644,
    )?;
    let mut output = fs::File::create(bundle.join("RUST-DEPENDENCIES.json"))?;
    output.set_permissions(fs::Permissions::from_mode(0o644))?;
    serde_json::to_writer_pretty(
        &mut output,
        &json!({"schema":1,"target":target,
        "scope":"Cargo metadata workspace normal and build edges, including resolved optional crates; excludes dev-only edges, system libraries and separately downloaded models/runtimes", "packages":inventory, "toolchain":toolchain}),
    )?;
    output.write_all(b"\n")?;
    Ok(())
}
