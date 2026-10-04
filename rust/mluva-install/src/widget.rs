//! The bundle installer is separate from the resident app: only an explicit
//! install command contacts Omarchy or replaces an owned widget.
mod files;
mod widget_args;

use files::{Result, copy_tree, file_hashes, pretty_json, read_json};
use serde_json::{Value, json};
use std::{
    env, fs,
    os::unix::fs::{MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    process::{Command, ExitCode},
    thread,
    time::Duration,
};

const ID: &str = "mluva.dictation";
const SOURCE: &str = "linux/quickshell/mluva.dictation";
const RECEIPT: &str = ".mluva-bundle.json";
const REPOSITORY: &str = "https://github.com/1vecera/Mluva";
const LEGACY_ORIGINS: [&str; 3] = [
    "https://github.com/1vecera/omarchy-mluva.git",
    "https://github.com/1vecera/omarchy-mluva",
    "git@github.com:1vecera/omarchy-mluva.git",
];

fn main() -> ExitCode {
    let mode = match widget_args::parse(env::args_os().skip(1)) {
        Ok(widget_args::Mode::Help) => {
            println!("{}\n\n{}", widget_args::USAGE, widget_args::HELP);
            return ExitCode::SUCCESS;
        }
        Ok(mode) => mode,
        Err(error) => {
            eprintln!(
                "{}\nmluva-install-widget: error: {error}",
                widget_args::USAGE
            );
            return ExitCode::from(2);
        }
    };
    match run(mode) {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Mluva widget setup: {error}");
            ExitCode::FAILURE
        }
    }
}

fn run(mode: widget_args::Mode) -> Result<()> {
    // Package layout is bin/ beside manifest.json and linux/quickshell/.
    // Never silently fall back to a development checkout or another release.
    let executable = env::current_exe()?;
    let repository = executable
        .parent()
        .and_then(Path::parent)
        .ok_or("The widget bundle is missing.")?;
    if let widget_args::Mode::Stage(target) = mode {
        return stage_widget(repository, &target);
    }
    let check = matches!(mode, widget_args::Mode::Install { check: true });
    let home = env::var_os("HOME")
        .filter(|value| !value.is_empty())
        .ok_or("HOME is unavailable.")?;
    let temporary = tempfile::Builder::new()
        .prefix("mluva-widget-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir()?;
    let source = temporary.path().join(ID);
    stage_widget(repository, &source)?;
    install_widget(&source, Path::new(&home), check)
}

fn stage_widget(repository: &Path, target: &Path) -> Result<()> {
    let source = repository.join(SOURCE);
    let manifest_path = repository.join("manifest.json");
    if source.is_symlink() || manifest_path.is_symlink() {
        return Err("The bundled widget source and manifest must not be symbolic links.".into());
    }
    file_hashes(&source)?;
    let mut manifest = read_json(&manifest_path)?;
    let entries = manifest
        .get_mut("entryPoints")
        .and_then(Value::as_object_mut)
        .ok_or("The widget entry points are invalid.")?;
    for path in entries.values_mut() {
        let relative = Path::new(
            path.as_str()
                .ok_or("The widget entry points are invalid.")?,
        )
        .strip_prefix(SOURCE)
        .map_err(|_| "The widget entry points are outside its bundle.")?;
        *path = json!(relative);
    }
    copy_tree(&source, target)?;
    fs::write(target.join("manifest.json"), pretty_json(&manifest)?)?;
    Ok(())
}

fn existing_plugin(plugins: &Path) -> Result<Option<PathBuf>> {
    if !plugins.exists() {
        return Ok(None);
    }
    let mut candidates = Vec::new();
    for item in fs::read_dir(plugins)? {
        let directory = item?.path();
        if directory
            .file_name()
            .is_some_and(|name| name.as_encoded_bytes().starts_with(b"."))
        {
            continue;
        }
        if directory.file_name().is_some_and(|name| name == ID)
            || read_json(&directory.join("manifest.json"))
                .is_ok_and(|manifest| manifest["id"] == ID)
        {
            candidates.push(directory);
        }
    }
    if candidates.len() > 1 {
        return Err("Multiple Mluva widgets found. Keep one installation before updating.".into());
    }
    Ok(candidates.pop())
}

fn check_existing(directory: Option<&Path>) -> Result<()> {
    let Some(directory) = directory else {
        return Ok(());
    };
    if directory.is_symlink() {
        return Err(format!("Preserved linked plugin: {}", directory.display()).into());
    }
    let receipt_path = directory.join(RECEIPT);
    if receipt_path.is_file() {
        let receipt = read_json(&receipt_path)?;
        if receipt["repository"] == REPOSITORY
            && receipt["sha256"] == json!(file_hashes(directory)?)
        {
            return Ok(());
        }
    } else if directory.join(".git").is_dir() {
        let git = |args: &[&str]| -> Result<String> {
            let result = Command::new("git")
                .arg("-C")
                .arg(directory)
                .args(args)
                .output()?;
            if !result.status.success() {
                return Err("The widget's local Git ownership check failed.".into());
            }
            Ok(String::from_utf8(result.stdout)?.trim().to_owned())
        };
        if LEGACY_ORIGINS.contains(&git(&["remote", "get-url", "origin"])?.as_str())
            && git(&["status", "--porcelain", "--untracked-files=all"])?.is_empty()
            && git(&["rev-list", "HEAD", "--not", "--remotes=origin"])?.is_empty()
        {
            return Ok(());
        }
    }
    Err(format!("Preserved unmanaged or locally edited widget: {}. Back it up outside the plugins directory before retrying, or use --app-only.", directory.display()).into())
}

fn checked(command: &mut Command) -> Result<()> {
    if command.status()?.success() {
        Ok(())
    } else {
        Err("A widget setup command failed.".into())
    }
}

fn validate(source: &Path) -> Result<()> {
    checked(
        Command::new("omarchy")
            .args(["plugin", "validate"])
            .arg(source),
    )
}

fn rescan() -> Result<()> {
    checked(Command::new("omarchy-shell").args(["shell", "rescanPlugins"]))
}

fn install_widget(source: &Path, home: &Path, check_only: bool) -> Result<()> {
    let plugins = home.join(".config/omarchy/plugins");
    let target = plugins.join(ID);
    let previous = existing_plugin(&plugins)?;
    check_existing(previous.as_deref())?;
    let mut manifest = read_json(&source.join("manifest.json"))?;
    if manifest["id"] != ID {
        return Err("The bundled widget has an unexpected identity.".into());
    }
    let version = manifest["version"]
        .as_str()
        .ok_or("The widget version is invalid.")?
        .to_owned();
    let hashes = file_hashes(source)?;
    validate(source)?;
    if check_only {
        return Ok(());
    }

    // This byte serialization is the released receipt contract, including ASCII
    // escaping and spaces. Keep existing bundle URLs stable for unchanged QML.
    let digest = files::hash_list_json(&hashes)?;
    let content_id = files::sha256(digest);
    let bundle_name = format!("bundle-{}", &content_id[..16]);
    let parent = plugins
        .parent()
        .ok_or("The widget destination is invalid.")?;
    fs::create_dir_all(parent)?;
    let temporary = tempfile::Builder::new()
        .prefix(".mluva-stage-")
        .permissions(fs::Permissions::from_mode(0o700))
        .tempdir_in(parent)?;
    let stage = temporary.path().join(ID);
    fs::create_dir(&stage)?;
    let bundle = stage.join(&bundle_name);
    copy_tree(source, &bundle)?;
    fs::remove_file(bundle.join("manifest.json"))?;
    for path in manifest
        .get_mut("entryPoints")
        .and_then(Value::as_object_mut)
        .ok_or("The widget entry points are invalid.")?
        .values_mut()
    {
        *path = json!(format!(
            "{bundle_name}/{}",
            path.as_str()
                .ok_or("The widget entry points are invalid.")?
        ));
    }
    fs::write(stage.join("manifest.json"), pretty_json(&manifest)?)?;
    fs::write(
        stage.join(RECEIPT),
        pretty_json(&json!({"repository": REPOSITORY, "sha256": file_hashes(&stage)?}))?,
    )?;
    validate(&stage)?;
    let staged_identity = fs::symlink_metadata(&stage)?;
    fs::create_dir_all(&plugins)?;
    let backup = if let Some(previous) = &previous {
        let root = parent.join("plugin-backups");
        fs::create_dir_all(&root)?;
        let folder = tempfile::Builder::new()
            .prefix(&format!("{ID}-"))
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root)?;
        let destination = folder.path().join(
            previous
                .file_name()
                .ok_or("The widget destination is invalid.")?,
        );
        fs::rename(previous, &destination)?;
        let _backup_directory = folder.keep(); // Backups survive outside discovery.
        Some(destination)
    } else {
        None
    };
    let mut promoted = false;
    let result = (|| {
        fs::rename(&stage, &target)?;
        promoted = true;
        rescan()?;
        let mut discovered = false;
        for _ in 0..40 {
            let result = Command::new("omarchy")
                .args(["plugin", "list", "--json"])
                .output()?;
            if !result.status.success() {
                return Err("A widget setup command failed.".into());
            }
            let list: Value = serde_json::from_slice(&result.stdout)
                .map_err(|_| "Omarchy returned an invalid plugin list.")?;
            if list
                .as_array()
                .ok_or("Omarchy returned an invalid plugin list.")?
                .iter()
                .any(|plugin| plugin["id"] == ID)
            {
                discovered = true;
                break;
            }
            thread::sleep(Duration::from_millis(50));
        }
        if !discovered {
            return Err("Omarchy did not discover the installed widget.".into());
        }
        checked(Command::new("omarchy").args(["plugin", "enable", ID]))
    })();
    if let Err(error) = result {
        // A failed promotion must never remove a directory that we did not put
        // there. Restore a renamed legacy installation under its original name.
        let restored = (|| -> Result<()> {
            if promoted {
                let current = fs::symlink_metadata(&target)?;
                if (current.dev(), current.ino()) != (staged_identity.dev(), staged_identity.ino())
                {
                    return Err("The widget destination changed during installation.".into());
                }
                fs::remove_dir_all(&target)?;
            }
            if let (Some(backup), Some(previous)) = (&backup, &previous) {
                fs::rename(backup, previous)?;
            }
            Ok(())
        })();
        let _ = rescan();
        restored.map_err(|_| match &backup {
            Some(path) => format!(
                "Widget rollback could not finish. The previous widget backup was preserved at {}.",
                path.display()
            ),
            None => {
                "Widget rollback could not finish. The replacement path was preserved.".to_owned()
            }
        })?;
        return Err(error);
    }
    println!(
        "Installed bundled widget {} at {}",
        version,
        target.display()
    );
    if let Some(backup) = backup {
        println!("Previous widget preserved at {}", backup.display());
    }
    Ok(())
}
