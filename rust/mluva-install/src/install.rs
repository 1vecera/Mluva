//! Install a prepared native bundle without changing user stores or enabling
//! desktop permissions. Legacy VoiceScribe migration is a separate prerequisite.
mod install_bundle;
mod install_paths;
mod install_process;
mod install_transaction;
use install_paths::{InstallPaths, Result, native_owner, regular, running};
use install_process::{checkpoint, run};
use install_transaction::{Snapshot, Transaction};
use std::{
    env, fs,
    io::{Read, Seek},
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

const LINKS: &[(&str, &str)] = &[
    ("mluva", "bin/mluva"),
    ("mluva-input-helper", "configure-input-helper.sh"),
    ("mluva-overlay", "configure-recording-overlay.sh"),
    ("mluva-uninstall", "uninstall.sh"),
    ("mluva-shell", "mluva-shell"),
    ("mluva-narrate", "mluva-narrate"),
    ("mluva-screenshot-editor", "mluva-screenshot-editor"),
];
fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}
fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}
fn executable(path: &Path) -> bool {
    std::ffi::CString::new(path.as_os_str().as_encoded_bytes()).is_ok_and(|path| unsafe {
        libc::faccessat(libc::AT_FDCWD, path.as_ptr(), libc::X_OK, libc::AT_EACCESS) == 0
    })
}
fn available(name: &str) -> bool {
    env::var_os("PATH").is_some_and(|paths| {
        env::split_paths(&paths)
            .any(|path| fs::metadata(path.join(name)).is_ok_and(|meta| meta.is_file()))
    })
}

fn preflight(paths: &InstallPaths) -> Result<()> {
    // Never partially migrate a historical installation or invoke its Python
    // migration fallback. This candidate cannot yet replace VoiceScribe.
    let managed = if paths.staged {
        paths.config.join("daniel-ai-skills")
    } else {
        secret_root(paths)
    };
    for path in [
        paths.data.join("voice-scribe"),
        paths.config.join("voice-scribe"),
        managed.join("env/voice-scribe.env"),
        paths
            .data
            .join("applications/com.voicescribe.Linux.desktop"),
        paths
            .data
            .join("icons/hicolor/scalable/apps/com.voicescribe.Linux.svg"),
        paths.config.join("autostart/com.voicescribe.Linux.desktop"),
        paths
            .data
            .join("gnome-shell/extensions/recording-status@voicescribe.local"),
        paths
            .data
            .join("gnome-shell/extensions/right-alt@voicescribe.local"),
        paths.config.join("omarchy/plugins/voice-scribe.dictation"),
    ]
    .into_iter()
    .chain(
        [
            "voice-scribe",
            "voice-scribe-input-helper",
            "voice-scribe-overlay",
            "voice-scribe-uninstall",
        ]
        .map(|name| paths.bin.join(name)),
    ) {
        if present(&path) {
            return Err("This native candidate cannot yet migrate VoiceScribe. The existing installation was left unchanged.".into());
        }
    }
    if !paths.staged && present(Path::new("/etc/systemd/system/voice-scribe-input@.service")) {
        return Err(
            "This native candidate cannot yet migrate the VoiceScribe system helper.".into(),
        );
    }
    for path in [&paths.app, &paths.data.join("mluva")] {
        if let Ok(meta) = path.symlink_metadata()
            && !meta.is_dir()
        {
            return Err(format!(
                "Refusing to replace an unexpected application path: {}",
                path.display()
            )
            .into());
        }
    }
    let marker = paths.app.join("pyproject.toml");
    if present(&paths.app)
        && !native_owner(&paths.app)
        && !(regular(&marker)
            && text(&marker)
                .split('\n')
                .any(|line| line == "name = \"mluva-linux\""))
    {
        return Err(format!(
            "Refusing to replace an unrecognized application directory: {}",
            paths.app.display()
        )
        .into());
    }
    for (name, target) in LINKS {
        let path = paths.bin.join(name);
        if !present(&path)
            || fs::read_link(&path).is_ok_and(|value| value == paths.app.join(target))
        {
            continue;
        }
        if *name == "mluva" && regular(&path) {
            let contents = text(&path);
            if contents
                .split('\n')
                .any(|line| line == format!("application_dir=\"{}\"", paths.app.display()))
                && contents.contains("-m mluva_linux.app")
            {
                continue;
            }
        }
        return Err(format!(
            "Refusing to replace an unrelated command: {}",
            path.display()
        )
        .into());
    }
    let desktop = paths.data.join("applications/com.mluva.Linux.desktop");
    if present(&desktop)
        && !(regular(&desktop)
            && text(&desktop).lines().any(|line| line == "Name=Mluva")
            && text(&desktop).lines().any(|line| {
                line == "Exec=mluva" || line == format!("Exec={}/mluva", paths.bin.display())
            }))
    {
        return Err(format!(
            "Refusing to replace an unrelated desktop entry: {}",
            desktop.display()
        )
        .into());
    }
    let icon = paths
        .data
        .join("icons/hicolor/scalable/apps/com.mluva.Linux.svg");
    if present(&icon)
        && !(regular(&icon) && text(&icon).contains("<title id=\"title\">Mluva</title>"))
    {
        return Err(format!(
            "Refusing to replace an unrelated application icon: {}",
            icon.display()
        )
        .into());
    }
    if running(&paths.app)? {
        return Err(format!("Mluva is running from {}. Close it before installation so its environment is not replaced in place.",paths.app.display()).into());
    }
    for name in ["pw-record", "pw-dump", "wl-copy"] {
        if !available(name) {
            return Err(format!("Missing required command: {name}").into());
        }
    }
    Ok(())
}

fn secret_root(paths: &InstallPaths) -> PathBuf {
    env::var_os("DAS_CONF_DIR")
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .unwrap_or_else(|| paths.config.join("daniel-ai-skills"))
}
fn profile(paths: &InstallPaths) -> Result<Option<(PathBuf, Vec<u8>)>> {
    if paths.staged {
        return Ok(None);
    }
    let root = secret_root(paths);
    let path = root.join("env/mluva.env");
    if fs::metadata(&path).is_ok_and(|meta| meta.len() > 0)
        || !executable(&root.join("bin/das-mcp-launch"))
        || !root.join("env/agent.env").is_file()
    {
        return Ok(None);
    }
    if present(&path) && !regular(&path) {
        return Err("Refusing to replace a linked or nonregular scoped credential profile.".into());
    }
    let catalog = fs::read(root.join("env/agent.env"))?;
    let prefix = b"DAS_ITEM_ELEVEN_LABS_API_KEY__CREDENTIAL=";
    let references: Vec<_> = catalog
        .split(|byte| *byte == b'\n')
        .filter_map(|line| line.strip_prefix(prefix))
        .collect();
    let valid = references.len() == 1
        && references[0]
            .strip_prefix(b"op://")
            .is_some_and(|reference| {
                let parts: Vec<_> = reference.splitn(3, |byte| *byte == b'/').collect();
                parts.len() == 3 && parts.iter().all(|part| !part.is_empty())
            });
    if !valid {
        return Err("Mluva could not resolve one reviewed ElevenLabs credential reference.".into());
    }
    let mut bytes = b"ELEVENLABS_API_KEY=".to_vec();
    bytes.extend_from_slice(references[0]);
    bytes.push(b'\n');
    Ok(Some((path, bytes)))
}

fn write(path: &Path, bytes: &[u8], mode: u32) -> Result<()> {
    fs::write(path, bytes)?;
    fs::set_permissions(path, fs::Permissions::from_mode(mode))?;
    Ok(())
}
fn quiet(command: &mut Command) -> &mut Command {
    command
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
}

fn hints(paths: &InstallPaths) -> Result<()> {
    println!(
        "Mluva is installed. Launch it from the application menu or run {}/mluva.",
        paths.bin.display()
    );
    println!(
        "On first launch, approve the F9 recording toggle and Ctrl+Alt+Escape cancellation shortcuts."
    );
    println!(
        "No logout is required for the application or shortcut changes. Change the recording key between F1 and F24 from the Capture page."
    );
    if paths.staged {
        println!(
            "Staged verification skipped live GNOME extension, systemd helper, accessibility, and secret-profile inspection."
        );
        return Ok(());
    }
    if env::var("XDG_CURRENT_DESKTOP")
        .unwrap_or_default()
        .to_lowercase()
        .contains("gnome")
    {
        if !available("gnome-extensions")
            || run(quiet(
                Command::new("gnome-extensions").args(["info", "recording-status@mluva.local"]),
            ))
            .is_err()
        {
            println!(
                "For a bottom recording bar that remains visible over other applications, install the optional display-only extension:\n  mluva-overlay install\nA newly installed GNOME Shell extension may require one logout and login before it can be enabled."
            );
        }
        if run(quiet(
            Command::new(paths.bin.join("mluva-input-helper")).arg("status"),
        ))
        .is_err()
        {
            println!(
                "For automatic paste in apps without native accessibility editing, install the optional keyboard-only helper:\n  mluva-input-helper install\nThis requires sudo once and grants same-user processes synthetic-keyboard access through an owner-only socket."
            );
        }
        if available("gsettings") {
            let mut output = tempfile::tempfile()?;
            let _ = run(quiet(Command::new("gsettings").args([
                "get",
                "org.gnome.desktop.interface",
                "toolkit-accessibility",
            ]))
            .stdout(output.try_clone()?));
            output.rewind()?;
            let mut value = String::new();
            output.take(4096).read_to_string(&mut value)?;
            if value.trim_end_matches('\n') != "true" {
                println!(
                    "Automatic insertion is unavailable while GNOME toolkit accessibility is off.\nEnable it before launching Mluva with: gsettings set org.gnome.desktop.interface toolkit-accessibility true\nApplications already open when it is enabled may need to be restarted before they expose text targets."
                );
            }
        }
    }
    let root = secret_root(paths);
    let populated = |name| fs::metadata(root.join(name)).is_ok_and(|meta| meta.len() > 0);
    if executable(&root.join("bin/das-mcp-launch")) && populated("env/mluva.env") {
        println!(
            "The launcher will resolve only the reviewed ElevenLabs credential reference at runtime."
        );
    } else if executable(&root.join("bin/das-agent-launch")) && populated("env/agent.env") {
        println!(
            "The launcher will use das-agent-launch --only for the selected ElevenLabs credential at runtime."
        );
    } else {
        println!(
            "Choose your speech and rewrite providers in Settings and supply their credentials through the application environment."
        );
    }
    Ok(())
}

fn install(source: &Path) -> Result<()> {
    let paths = InstallPaths::from_environment("install")?;
    preflight(&paths)?;
    install_bundle::validate(source)?;
    let scoped = profile(&paths)?;
    let desktop = paths.data.join("applications/com.mluva.Linux.desktop");
    let icon = paths
        .data
        .join("icons/hicolor/scalable/apps/com.mluva.Linux.svg");
    let targets: Vec<_> = [paths.app.clone(), desktop.clone(), icon.clone()]
        .into_iter()
        .chain(LINKS.iter().map(|(name, _)| paths.bin.join(name)))
        .chain(scoped.as_ref().map(|(path, _)| path.clone()))
        .collect();
    let snapshots = targets
        .iter()
        .map(|path| Snapshot::read(path).map(|snapshot| (path.clone(), snapshot)))
        .collect::<Result<Vec<_>>>()?;
    let mut transaction = Transaction::default();
    transaction.directory(paths.app.parent().ok_or("Missing app parent")?, 0o700, true)?;
    for directory in [
        &paths.bin,
        desktop.parent().ok_or("Missing desktop parent")?,
        icon.parent().ok_or("Missing icon parent")?,
    ] {
        transaction.directory(directory, 0o755, false)?;
    }
    if let Some((path, _)) = &scoped {
        transaction.directory(path.parent().ok_or("Missing profile parent")?, 0o700, true)?;
    }
    let staged = transaction.prepare(&paths.app)?;
    install_bundle::copy(source, &staged)?;
    install_bundle::validate(&staged)?;
    // Load actual linked native dependencies, without initializing user stores,
    // the desktop, providers, or the managed credential launchers.
    run(quiet(
        Command::new(staged.join("bin/mluva"))
            .arg("--help")
            .env("MLUVA_SECRET_PROFILE", "1"),
    ))?;
    checkpoint()?;
    for (name, target) in LINKS {
        symlink(
            paths.app.join(target),
            transaction.prepare(&paths.bin.join(name))?,
        )?;
    }
    let entry = fs::read_to_string(staged.join("resources/com.mluva.Linux.desktop.in"))?
        .replace("@EXECUTABLE@", &paths.bin.join("mluva").to_string_lossy());
    write(&transaction.prepare(&desktop)?, entry.as_bytes(), 0o644)?;
    write(
        &transaction.prepare(&icon)?,
        &fs::read(staged.join("resources/com.mluva.Linux.svg"))?,
        0o644,
    )?;
    if let Some((path, bytes)) = &scoped {
        write(&transaction.prepare(path)?, bytes, 0o600)?;
    }
    if running(&paths.app)? {
        return Err(
            "Mluva started while preparing this upgrade. Close it before installation.".into(),
        );
    }
    for (path, snapshot) in &snapshots {
        transaction.publish(path, snapshot)?;
    }
    if available("update-desktop-database") {
        run(Command::new("update-desktop-database")
            .arg(desktop.parent().ok_or("Missing desktop parent")?)
            .stdin(Stdio::null()))?;
    }
    transaction.commit()?;
    if scoped.is_some() {
        println!("Configured the scoped Mluva secret profile.");
    }
    hints(&paths)?;
    checkpoint()
}

fn main() -> ExitCode {
    let arguments: Vec<_> = env::args_os().skip(1).collect();
    if arguments.len() == 1 && matches!(arguments[0].to_str(), Some("--help" | "-h")) {
        println!(
            "Usage: mluva-install\n\nInstall this prepared native Mluva bundle for the current user.\nSet MLUVA_INSTALL_HOME to an isolated prefix for staged verification."
        );
        return ExitCode::SUCCESS;
    }
    if !arguments.is_empty() {
        eprintln!("Usage: mluva-install");
        return ExitCode::from(2);
    }
    let result = (|| {
        install_process::watch_signals()?;
        let executable = env::current_exe()?;
        install(
            executable
                .parent()
                .and_then(Path::parent)
                .ok_or("Missing native bundle")?,
        )
    })();
    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Mluva install: {error}");
            ExitCode::from(
                error
                    .downcast_ref::<install_process::Failed>()
                    .map_or(1, |failure| failure.0),
            )
        }
    }
}
