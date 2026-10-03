//! Remove only this user's application and owned desktop integrations.
mod install_paths;
use install_paths::{InstallPaths, Result, native_owner, regular, running};
use std::{
    env, fs,
    os::unix::process::ExitStatusExt,
    path::{Path, PathBuf},
    process::{Command, ExitCode, Stdio},
};

fn text(path: &Path) -> String {
    fs::read_to_string(path).unwrap_or_default()
}

fn command_available(name: &str) -> bool {
    // Bash reports an existing non-executable candidate too; invocation must
    // retain exit 126. Command still searches later executable PATH entries.
    env::var_os("PATH").is_some_and(|value| {
        env::split_paths(&value)
            .any(|path| fs::metadata(path.join(name)).is_ok_and(|metadata| metadata.is_file()))
    })
}

fn code(command: &mut Command) -> u8 {
    match command.status() {
        Ok(status) => match status.signal() {
            Some(signal) => {
                eprintln!("Mluva desktop helper stopped after signal {signal}.");
                (128 + signal) as u8
            }
            None => status.code().unwrap_or(1) as u8,
        },
        Err(error) => {
            eprintln!(
                "Mluva could not execute {}.",
                command.get_program().to_string_lossy()
            );
            if error.kind() == std::io::ErrorKind::NotFound {
                127
            } else {
                126
            }
        }
    }
}

fn remove_link(path: &Path, target: &Path) -> Result<()> {
    if let Ok(actual) = fs::read_link(path) {
        if actual == target {
            fs::remove_file(path)?;
        } else {
            eprintln!("Preserved unexpected symlink at {}.", path.display());
        }
    }
    Ok(())
}

fn extension_owned(source: &Path, installed: &Path) -> Result<bool> {
    let mut expected = fs::read_dir(source)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    let mut actual = fs::read_dir(installed)?
        .map(|entry| entry.map(|entry| entry.file_name()))
        .collect::<std::io::Result<Vec<_>>>()?;
    expected.sort();
    actual.sort();
    if expected != actual {
        return Ok(false);
    }
    for name in expected {
        let path = installed.join(&name);
        if !regular(&path) || fs::read(path)? != fs::read(source.join(name))? {
            return Ok(false);
        }
    }
    Ok(true)
}

fn uninstall(source: &Path) -> Result<u8> {
    let paths = InstallPaths::from_environment("uninstall")?;
    if running(&paths.app)? {
        return Err(format!(
            "Mluva is running from {}. Close it before uninstalling.",
            paths.app.display()
        )
        .into());
    }
    if paths.app.is_symlink() || paths.data.join("mluva").is_symlink() {
        return Err("Refusing to remove a symlinked application directory.".into());
    }
    let marker = paths.app.join("pyproject.toml");
    if paths.app.is_dir()
        && !native_owner(&paths.app)
        && !(regular(&marker)
            && text(&marker)
                .lines()
                .any(|line| line == "name = \"mluva-linux\""))
    {
        return Err(format!(
            "Refusing to remove an unrecognized application directory: {}",
            paths.app.display()
        )
        .into());
    }
    if paths.staged {
        println!("Staged verification skipped live GNOME extension and systemd helper inspection.");
    } else {
        if Path::new("/etc/systemd/system/mluva-input@.service").is_file()
            && code(Command::new(source.join("configure-input-helper.sh")).arg("remove")) != 0
        {
            return Err("The application remains installed because its system input helper could not be removed.".into());
        }
        if command_available("gnome-extensions")
            && Command::new("gnome-extensions")
                .args(["info", "recording-status@mluva.local"])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .status()
                .is_ok_and(|status| status.success())
            && code(Command::new(source.join("configure-recording-overlay.sh")).arg("remove")) != 0
        {
            eprintln!(
                "Warning: the optional Mluva recording overlay could not be removed automatically."
            );
            eprintln!(
                "Remove it later with: gnome-extensions uninstall recording-status@mluva.local"
            );
        }
    }
    let extension = paths
        .data
        .join("gnome-shell/extensions/recording-status@mluva.local");
    let packaged = source.join("gnome-extension/recording-status@mluva.local");
    if extension.is_dir() && !extension.is_symlink() && packaged.is_dir() {
        // An unreadable or nonregular extension remains user-owned.
        if extension_owned(&packaged, &extension).unwrap_or(false) {
            fs::remove_dir_all(&extension)?;
        } else {
            eprintln!(
                "Preserved a modified recording extension at {}.",
                extension.display()
            );
        }
    }
    let launcher = paths.bin.join("mluva");
    if regular(&launcher) {
        let contents = text(&launcher);
        if contents
            .lines()
            .any(|line| line == format!("application_dir=\"{}\"", paths.app.display()))
            && contents.contains("-m mluva_linux.app")
        {
            fs::remove_file(&launcher)?;
        } else {
            eprintln!("Preserved unexpected executable at {}.", launcher.display());
        }
    } else if fs::read_link(&launcher).is_ok_and(|target| target == paths.app.join("bin/mluva")) {
        fs::remove_file(&launcher)?;
    }
    for (name, target) in [
        ("mluva-input-helper", "configure-input-helper.sh"),
        ("mluva-overlay", "configure-recording-overlay.sh"),
        ("mluva-uninstall", "uninstall.sh"),
        ("mluva-shell", "mluva-shell"),
        ("mluva-narrate", "mluva-narrate"),
        ("mluva-screenshot-editor", "mluva-screenshot-editor"),
        ("tensaku-edit", "mluva-screenshot-editor"),
    ] {
        remove_link(&paths.bin.join(name), &paths.app.join(target))?;
    }
    let applications = paths.data.join("applications");
    for entry in [
        applications.join("com.mluva.Linux.desktop"),
        paths.config.join("autostart/com.mluva.Linux.desktop"),
    ] {
        if regular(&entry) {
            let contents = text(&entry);
            if contents.lines().any(|line| line == "Name=Mluva")
                && contents.lines().any(|line| {
                    line == "Exec=mluva"
                        || line == format!("Exec={}", paths.bin.join("mluva").display())
                })
            {
                fs::remove_file(entry)?;
            }
        }
    }
    let icon = paths
        .data
        .join("icons/hicolor/scalable/apps/com.mluva.Linux.svg");
    if icon.is_file() && text(&icon).contains("<title id=\"title\">Mluva</title>") {
        fs::remove_file(icon)?;
    }
    if paths.app.is_dir() {
        fs::remove_dir_all(&paths.app)?;
    }
    if command_available("update-desktop-database") && applications.is_dir() {
        let status = code(Command::new("update-desktop-database").arg(applications));
        if status != 0 {
            return Ok(status);
        }
    }
    println!("Mluva application files and registered desktop integrations were removed.");
    println!(
        "Settings and user-created history remain under {}/mluva and {}/mluva.",
        paths.config.display(),
        paths.data.display()
    );
    Ok(0)
}

fn main() -> ExitCode {
    let result = (|| {
        let binary = env::current_exe()?;
        let source: PathBuf = binary
            .parent()
            .and_then(Path::parent)
            .ok_or("Missing native application directory.")?
            .to_owned();
        uninstall(&source)
    })();
    match result {
        Ok(status) => ExitCode::from(status),
        Err(error) => {
            eprintln!("{error}");
            ExitCode::FAILURE
        }
    }
}
