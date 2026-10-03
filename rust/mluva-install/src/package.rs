//! Assemble only the reviewed native runtime. Development peers, Python
//! implementations, model caches and user data are never package inputs.
mod package_licenses;
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use std::{
    collections::BTreeMap,
    env, fs,
    io::{Read, Write},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const BINARIES: &[&str] = &[
    "mluva",
    "mluva-shell",
    "mluva-narrate",
    "mluva-asr-worker",
    "mluva-audio-cleanup",
    "mluva-install-widget",
    "mluva-screenshot-editor",
    "mluva-uninstall",
    "mluva-install",
];

fn main() -> ExitCode {
    let mut args: Vec<_> = env::args_os().skip(1).collect();
    let archive = args.first().is_some_and(|value| value == "--archive");
    if archive {
        args.remove(0);
    }
    let usage = "Usage: mluva-package [--archive] SOURCE_DIRECTORY [BINARY_DIRECTORY] OUTPUT_PATH";
    if args.len() == 1 && matches!(args[0].to_str(), Some("-h" | "--help")) {
        println!(
            "{usage}\n\nAssemble a native runtime directory, or a reproducible tar.gz with --archive.\nBinaries default to the directory containing this package builder.\nUses the source lockfile and cached crate archives for dependency notices.\nThe output must not exist; no application is installed or started."
        );
        return ExitCode::SUCCESS;
    }
    if !matches!(args.len(), 2 | 3) {
        eprintln!("{usage}");
        return ExitCode::from(2);
    }
    let result: Result<()> = (|| {
        let binaries = if args.len() == 3 {
            PathBuf::from(&args[1])
        } else {
            env::current_exe()?
                .parent()
                .ok_or("The built executable directory is unavailable.")?
                .to_owned()
        };
        package(
            Path::new(&args[0]),
            &binaries,
            Path::new(args.last().expect("output argument was checked")),
            archive,
        )
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Mluva package: {error}");
            ExitCode::FAILURE
        }
    }
}

fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}

fn regular(path: &Path) -> Result<()> {
    if !path.symlink_metadata()?.is_file() {
        return Err(format!("Expected a regular package input: {}", path.display()).into());
    }
    Ok(())
}

fn binary(path: &Path) -> Result<()> {
    regular(path)?;
    if fs::metadata(path)?.permissions().mode() & 0o111 == 0 {
        return Err(format!("The native binary is not executable: {}", path.display()).into());
    }
    let mut header = [0; 20];
    fs::File::open(path)?.read_exact(&mut header)?;
    if &header[..4] != b"\x7fELF"
        || header[4] != 2
        || header[5] != 1
        || !matches!(header[16], 2 | 3)
        || header[17] != 0
    {
        return Err(format!(
            "Expected a native 64-bit Linux executable: {}",
            path.display()
        )
        .into());
    }
    let machine = u16::from_le_bytes([header[18], header[19]]);
    let expected = if cfg!(target_arch = "x86_64") {
        62
    } else if cfg!(target_arch = "aarch64") {
        183
    } else {
        0
    };
    if expected == 0 || machine != expected {
        return Err("The native binary architecture differs from this package builder.".into());
    }
    Ok(())
}

fn copy_file(source: &Path, target: &Path, mode: u32) -> Result<()> {
    regular(source)?;
    fs::create_dir_all(target.parent().ok_or("Missing package directory")?)?;
    fs::copy(source, target)?;
    fs::set_permissions(target, fs::Permissions::from_mode(mode))?;
    Ok(())
}

fn assets(source: &Path, target: &Path) -> Result<()> {
    if !source.symlink_metadata()?.is_dir() {
        return Err("Package asset roots must be real directories.".into());
    }
    fs::create_dir_all(target)?;
    for item in fs::read_dir(source)? {
        let item = item?;
        if matches!(
            item.file_name().to_str(),
            Some(".git" | ".gitignore" | ".gitattributes")
        ) {
            continue;
        }
        let from = item.path();
        let to = target.join(item.file_name());
        if item.file_type()?.is_dir() {
            assets(&from, &to)?;
        } else {
            let extension = from
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or("");
            let license = matches!(
                from.file_name().and_then(|value| value.to_str()),
                Some("LICENSE" | "NOTICE")
            );
            if !license
                && !matches!(
                    extension,
                    "json" | "qml" | "js" | "css" | "svg" | "ttf" | "txt" | "md" | "html"
                )
            {
                return Err(format!("Unexpected runtime asset: {}", from.display()).into());
            }
            copy_file(&from, &to, 0o644)?;
        }
    }
    fs::set_permissions(target, fs::Permissions::from_mode(0o755))?;
    Ok(())
}

fn hashes(root: &Path) -> Result<BTreeMap<String, String>> {
    fn walk(root: &Path, path: &Path, output: &mut BTreeMap<String, String>) -> Result<()> {
        for entry in fs::read_dir(path)? {
            let entry = entry?;
            let path = entry.path();
            if entry.file_type()?.is_dir() {
                walk(root, &path, output)?;
            } else if entry.file_type()?.is_file() {
                let mut input = fs::File::open(&path)?;
                let mut hash = Sha256::new();
                let mut bytes = [0; 65536];
                loop {
                    let size = input.read(&mut bytes)?;
                    if size == 0 {
                        break;
                    }
                    hash.update(&bytes[..size]);
                }
                output.insert(
                    path.strip_prefix(root)?
                        .to_str()
                        .ok_or("Invalid package filename")?
                        .to_owned(),
                    hash.finalize()
                        .iter()
                        .map(|byte| format!("{byte:02x}"))
                        .collect(),
                );
            }
        }
        Ok(())
    }
    let mut output = BTreeMap::new();
    walk(root, root, &mut output)?;
    Ok(output)
}

fn archive(root: &Path, target: &Path) -> Result<()> {
    fn append<W: Write>(
        builder: &mut tar::Builder<W>,
        root: &Path,
        path: &Path,
        prefix: &Path,
    ) -> Result<()> {
        builder.append_path_with_name(path, prefix.join(path.strip_prefix(root)?))?;
        if path.symlink_metadata()?.is_dir() {
            let mut entries = fs::read_dir(path)?.collect::<std::io::Result<Vec<_>>>()?;
            entries.sort_by_key(|entry| entry.file_name());
            for entry in entries {
                append(builder, root, &entry.path(), prefix)?;
            }
        }
        Ok(())
    }
    let file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(target)?;
    let gzip = flate2::GzBuilder::new()
        .mtime(0)
        .write(file, flate2::Compression::best());
    let mut builder = tar::Builder::new(gzip);
    builder.mode(tar::HeaderMode::Deterministic);
    builder.follow_symlinks(false);
    append(
        &mut builder,
        root,
        root,
        Path::new(&format!("mluva-{}", env!("CARGO_PKG_VERSION"))),
    )?;
    let file = builder.into_inner()?.finish()?;
    file.set_permissions(fs::Permissions::from_mode(0o644))?;
    file.sync_all()?;
    Ok(())
}

fn package(source: &Path, binaries: &Path, output: &Path, compressed: bool) -> Result<()> {
    let source = source.canonicalize()?;
    let binaries = binaries.canonicalize()?;
    let output = std::path::absolute(output)?;
    if present(&output) {
        return Err("The package output already exists; it was left untouched.".into());
    }
    let manifest: Value = serde_json::from_slice(&fs::read(source.join("manifest.json"))?)
        .map_err(|_| "The release manifest is invalid.")?;
    if manifest["id"] != "mluva.dictation" || manifest["version"] != env!("CARGO_PKG_VERSION") {
        return Err("The app and bundled widget must have the same release version.".into());
    }
    for name in BINARIES {
        binary(&binaries.join(name))?;
    }
    let parent = output
        .parent()
        .ok_or("The package output needs a parent directory.")?;
    fs::create_dir_all(parent)?;
    let parent = parent.canonicalize()?;
    for path in [
        "rust/mluva-gtk/resources",
        "linux/quickshell/mluva.dictation",
        "linux/gnome-extension",
    ] {
        if parent.starts_with(source.join(path).canonicalize()?) {
            return Err("The package output must be outside its source asset directories.".into());
        }
    }
    let temporary = tempfile::Builder::new()
        .prefix(".mluva-package-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(&parent)?;
    let bundle = temporary.path().join("bundle");
    fs::create_dir(&bundle)?;
    for relative in [
        "bin",
        "linux",
        "linux/quickshell",
        "linux/integrations",
        "linux/integrations/tensaku",
        "quickshell",
    ] {
        let directory = bundle.join(relative);
        fs::create_dir_all(&directory)?;
        fs::set_permissions(directory, fs::Permissions::from_mode(0o755))?;
    }
    for name in BINARIES {
        copy_file(&binaries.join(name), &bundle.join("bin").join(name), 0o755)?;
    }
    for name in ["LICENSE", "manifest.json"] {
        copy_file(&source.join(name), &bundle.join(name), 0o644)?;
    }
    assets(
        &source.join("rust/mluva-gtk/resources"),
        &bundle.join("resources"),
    )?;
    assets(
        &source.join("linux/quickshell/mluva.dictation"),
        &bundle.join("linux/quickshell/mluva.dictation"),
    )?;
    assets(
        &source.join("linux/gnome-extension"),
        &bundle.join("gnome-extension"),
    )?;
    for name in ["mluva-input@.service", "com.mluva.Linux.desktop.in"] {
        copy_file(
            &source.join("linux/resources").join(name),
            &bundle.join("resources").join(name),
            0o644,
        )?;
    }
    for name in [
        "configure-input-helper.sh",
        "configure-recording-overlay.sh",
    ] {
        let original = source.join("linux").join(name);
        if fs::read_to_string(&original)?.contains("/usr/bin/python") {
            return Err("A desktop helper still requires Python.".into());
        }
        copy_file(&original, &bundle.join(name), 0o755)?;
    }
    for name in ["install-narrated-editor.sh", "build-narrated-editor.sh"] {
        copy_file(
            &source.join("linux").join(name),
            &bundle.join("linux").join(name),
            0o755,
        )?;
    }
    for name in [
        "LICENSE",
        "NOTICE",
        "README.md",
        "upstream-commit",
        "narration.patch",
    ] {
        let relative = Path::new("linux/integrations/tensaku").join(name);
        copy_file(&source.join(&relative), &bundle.join(&relative), 0o644)?;
    }
    let result = Command::new(bundle.join("bin/mluva-install-widget"))
        .arg("--stage")
        .arg(bundle.join("quickshell/mluva.dictation"))
        .stdin(Stdio::null())
        .output()?;
    if !result.status.success() {
        return Err("The native widget could not be staged.".into());
    }
    let widget_manifest = bundle.join("quickshell/mluva.dictation/manifest.json");
    regular(&widget_manifest)?;
    fs::set_permissions(widget_manifest, fs::Permissions::from_mode(0o644))?;
    // Keep the historical editor/bridge paths used by existing managed links.
    for name in ["mluva-shell", "mluva-narrate", "mluva-screenshot-editor"] {
        symlink(PathBuf::from("bin").join(name), bundle.join(name))?;
    }
    symlink("bin/mluva-uninstall", bundle.join("uninstall.sh"))?;
    copy_file(
        &source.join("install.sh"),
        &bundle.join("install.sh"),
        0o755,
    )?;
    for (source_name, target) in [
        ("app-install.sh", "linux/install.sh"),
        ("app-uninstall.sh", "linux/uninstall.sh"),
    ] {
        copy_file(
            &source
                .join("rust/mluva-install/resources")
                .join(source_name),
            &bundle.join(target),
            0o755,
        )?;
    }
    package_licenses::stage(&source, &bundle)?;
    let content_hashes = hashes(&bundle)?;
    let mut inventory = fs::File::create(bundle.join(".mluva-native.json"))?;
    inventory.set_permissions(fs::Permissions::from_mode(0o644))?;
    serde_json::to_writer_pretty(
        &mut inventory,
        &json!({
            "schema":1,"application":"com.mluva.Linux","implementation":"rust",
            "version":env!("CARGO_PKG_VERSION"),"sha256":content_hashes,
            "links":{"mluva-shell":"bin/mluva-shell","mluva-narrate":"bin/mluva-narrate","mluva-screenshot-editor":"bin/mluva-screenshot-editor","uninstall.sh":"bin/mluva-uninstall"}
        }),
    )?;
    inventory.write_all(b"\n")?;
    inventory.sync_all()?;
    fs::set_permissions(&bundle, fs::Permissions::from_mode(0o755))?;
    let publication = if compressed {
        let path = temporary.path().join("runtime.tar.gz");
        archive(&bundle, &path)?;
        path
    } else {
        bundle
    };
    // RENAME_NOREPLACE closes the ownership race between preflight and publication.
    let from = std::ffi::CString::new(publication.as_os_str().as_encoded_bytes())?;
    let to = std::ffi::CString::new(output.as_os_str().as_encoded_bytes())?;
    if unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            from.as_ptr(),
            libc::AT_FDCWD,
            to.as_ptr(),
            libc::RENAME_NOREPLACE,
        )
    } != 0
    {
        return Err(std::io::Error::last_os_error().into());
    }
    println!(
        "Native Mluva {} {} created at {}",
        env!("CARGO_PKG_VERSION"),
        if compressed { "archive" } else { "bundle" },
        output.display()
    );
    Ok(())
}
