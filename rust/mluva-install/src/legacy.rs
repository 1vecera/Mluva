//! One-time identity migration at the installation boundary. Legacy names are
//! accepted as owned inputs; no legacy code is installed or executed.
mod desktop;
mod state;
use crate::{
    install_paths::{InstallPaths, Result, native_owner, regular, running},
    install_process::{Rollback, checkpoint},
    install_transaction::{Snapshot, Transaction},
};
use std::{
    fs,
    os::unix::fs::{PermissionsExt, symlink},
    path::{Path, PathBuf},
};

const OLD: &str = "voice-scribe";
const OLD_DESKTOP: &str = "com.voicescribe.Linux";
const OLD_EXTENSIONS: [&str; 2] = [
    "recording-status@voicescribe.local",
    "right-alt@voicescribe.local",
];
const EXTENSION: &str = "recording-status@mluva.local";
const COMMANDS: [(&str, Option<&str>); 5] = [
    ("mluva", None),
    ("mluva-input-helper", Some("configure-input-helper.sh")),
    ("mluva-overlay", Some("configure-recording-overlay.sh")),
    ("mluva-uninstall", Some("uninstall.sh")),
    ("mluva-shell", Some("mluva-shell")),
];
const OLD_COMMANDS: [&str; 3] = [
    "voice-scribe",
    "voice-scribe-input-helper",
    "voice-scribe-overlay",
];

fn present(path: &Path) -> bool {
    path.symlink_metadata().is_ok()
}
fn text(path: &Path) -> Result<String> {
    fs::read_to_string(path)
        .map(|value| value.replace("\r\n", "\n").replace('\r', "\n"))
        .map_err(|_| "Mluva could not read a migration text file.".into())
}
fn require_regular(path: &Path) -> Result<()> {
    if present(path) && !regular(path) {
        return Err(format!("Refusing an unexpected file: {}", path.display()).into());
    }
    Ok(())
}

/// Preserve links and modes without opening any link target or special file.
fn copy(source: &Path, target: &Path) -> Result<()> {
    let meta = source.symlink_metadata()?;
    fs::create_dir_all(target.parent().ok_or("Missing backup parent")?)?;
    if meta.is_symlink() {
        symlink(fs::read_link(source)?, target)?;
    } else if meta.is_dir() {
        fs::create_dir(target)?;
        for entry in fs::read_dir(source)? {
            let entry = entry?;
            copy(&entry.path(), &target.join(entry.file_name()))?;
        }
        fs::set_permissions(target, meta.permissions())?;
    } else if meta.is_file() {
        fs::copy(source, target)?;
    } else {
        return Err(format!("Refusing to snapshot a special file: {}", source.display()).into());
    }
    Ok(())
}

struct Legacy<'a> {
    source: &'a Path,
    paths: &'a InstallPaths,
    old_data: PathBuf,
    new_data: PathBuf,
    old_config: PathBuf,
    new_config: PathBuf,
    secret: PathBuf,
    extensions: PathBuf,
    binding: PathBuf,
    autostart: PathBuf,
    plugin: PathBuf,
    desktop: desktop::Desktop,
}

impl<'a> Legacy<'a> {
    fn new(source: &'a Path, paths: &'a InstallPaths) -> Self {
        Self {
            source,
            paths,
            old_data: paths.data.join(OLD),
            new_data: paths.data.join("mluva"),
            old_config: paths.config.join(OLD),
            new_config: paths.config.join("mluva"),
            secret: if paths.staged {
                paths.config.join("daniel-ai-skills")
            } else {
                crate::secret_root(paths)
            }
            .join("env"),
            extensions: paths.data.join("gnome-shell/extensions"),
            binding: paths.config.join("hypr/bindings.conf"),
            autostart: paths.config.join("autostart/com.voicescribe.Linux.desktop"),
            plugin: paths.config.join("omarchy/plugins/mluva.dictation"),
            desktop: desktop::Desktop::default(),
        }
    }
    fn needed(&self) -> bool {
        [
            self.old_data.clone(),
            self.old_config.clone(),
            self.secret.join("voice-scribe.env"),
            self.paths
                .data
                .join("applications/com.voicescribe.Linux.desktop"),
            self.paths
                .data
                .join("icons/hicolor/scalable/apps/com.voicescribe.Linux.svg"),
            self.autostart.clone(),
        ]
        .into_iter()
        .chain(OLD_COMMANDS.map(|name| self.paths.bin.join(name)))
        .chain(OLD_EXTENSIONS.map(|name| self.extensions.join(name)))
        .any(|path| present(&path))
            || (!self.paths.staged && present(Path::new(desktop::OLD_UNIT)))
    }
    fn preflight(&mut self) -> Result<()> {
        for root in [
            &self.old_data,
            &self.new_data,
            &self.old_config,
            &self.new_config,
        ] {
            if present(root) && !root.symlink_metadata()?.is_dir() {
                return Err(
                    format!("Refusing an unexpected state directory: {}", root.display()).into(),
                );
            }
        }
        for (old, new) in [
            (&self.old_data, &self.new_data),
            (&self.old_config, &self.new_config),
        ] {
            if present(old) && present(new) {
                return Err(format!(
                    "Both legacy and Mluva state exist; reconcile them before upgrading: {}, {}",
                    old.display(),
                    new.display()
                )
                .into());
            }
        }
        for root in [&self.old_data, &self.new_data] {
            let app = root.join("app");
            if !present(&app) {
                continue;
            }
            let marker = app.join("pyproject.toml");
            require_regular(&marker)?;
            let owned = native_owner(&app)
                || (regular(&marker)
                    && text(&marker)?.lines().any(|line| {
                        matches!(
                            line,
                            "name = \"mluva-linux\"" | "name = \"voice-scribe-linux\""
                        )
                    }));
            if app.is_symlink() || !owned {
                return Err(format!(
                    "Refusing an unrecognized application directory: {}",
                    app.display()
                )
                .into());
            }
            if running(&app)? {
                return Err(format!(
                    "Mluva is running from {}. Close it before installation.",
                    app.display()
                )
                .into());
            }
        }
        for name in COMMANDS.iter().map(|(name, _)| *name).chain(OLD_COMMANDS) {
            let path = self.paths.bin.join(name);
            if !present(&path) {
                continue;
            }
            let canonical = name.replace(OLD, "mluva");
            let helper = COMMANDS
                .iter()
                .find(|(name, _)| *name == canonical)
                .ok_or("Unknown migration command")?
                .1;
            let owned = if path.is_symlink() {
                let target = fs::read_link(&path)?;
                OLD_COMMANDS.contains(&name) && target == Path::new(&canonical)
                    || [&self.old_data, &self.new_data]
                        .iter()
                        .any(|root| target == root.join("app").join(helper.unwrap_or("bin/mluva")))
            } else if helper.is_none() && regular(&path) {
                let content = text(&path)?;
                [
                    (&self.old_data, "voice_scribe_linux"),
                    (&self.new_data, "mluva_linux"),
                ]
                .iter()
                .any(|(root, module)| {
                    content
                        .lines()
                        .any(|line| line == format!("application_dir=\"{}/app\"", root.display()))
                        && content.contains(&format!("-m {module}.app"))
                })
            } else {
                false
            };
            if !owned {
                return Err(format!(
                    "Refusing to replace an unrelated command: {}",
                    path.display()
                )
                .into());
            }
        }
        for identity in [OLD_DESKTOP, "com.mluva.Linux"] {
            let entry = self
                .paths
                .data
                .join("applications")
                .join(format!("{identity}.desktop"));
            let icon = self
                .paths
                .data
                .join("icons/hicolor/scalable/apps")
                .join(format!("{identity}.svg"));
            require_regular(&entry)?;
            require_regular(&icon)?;
            if present(&entry)
                && !text(&entry)?.lines().any(|line| {
                    [OLD, "mluva"]
                        .iter()
                        .any(|name| line == format!("Exec={}", self.paths.bin.join(name).display()))
                })
            {
                return Err(
                    format!("Refusing an unrelated desktop entry: {}", entry.display()).into(),
                );
            }
            if present(&icon)
                && !["Mluva", "Voice Scribe"].iter().any(|name| {
                    text(&icon).is_ok_and(|value| {
                        value.contains(&format!("<title id=\"title\">{name}</title>"))
                    })
                })
            {
                return Err(
                    format!("Refusing an unrelated application icon: {}", icon.display()).into(),
                );
            }
        }
        for name in OLD_EXTENSIONS {
            let extension = self.extensions.join(name);
            if !present(&extension) {
                continue;
            }
            let metadata = extension.join("metadata.json");
            require_regular(&metadata)?;
            let value: serde_json::Value = serde_json::from_slice(&fs::read(metadata)?)
                .map_err(|_| "The legacy extension metadata is invalid.")?;
            if extension.is_symlink() || value["uuid"] != name {
                return Err(
                    format!("Refusing an unrelated extension: {}", extension.display()).into(),
                );
            }
            if name == OLD_EXTENSIONS[0] && present(&self.extensions.join(EXTENSION)) {
                return Err(
                    "Both recording overlays are installed; reconcile them before upgrading."
                        .into(),
                );
            }
        }
        let (old_profile, new_profile) = (
            self.secret.join("voice-scribe.env"),
            self.secret.join("mluva.env"),
        );
        require_regular(&old_profile)?;
        require_regular(&new_profile)?;
        if present(&old_profile) {
            if !regex::Regex::new(r"\AELEVENLABS_API_KEY=op://[^/\n]+/[^/\n]+/[^\n]+\n?\z")?
                .is_match(&text(&old_profile)?)
            {
                return Err("The legacy credential profile is not a single reviewed reference; it was left untouched.".into());
            }
            if present(&new_profile) && fs::read(&old_profile)? != fs::read(&new_profile)? {
                return Err(
                    "The credential reference profiles differ; they were left untouched.".into(),
                );
            }
        }
        require_regular(&self.binding)?;
        require_regular(&self.autostart)?;
        if present(&self.autostart) {
            if present(&self.autostart.with_file_name("com.mluva.Linux.desktop")) {
                return Err(
                    "Both autostart entries exist; reconcile them before upgrading.".into(),
                );
            }
            if !text(&self.autostart)?.lines().any(|line| {
                [
                    OLD.to_owned(),
                    "mluva".to_owned(),
                    self.paths.bin.join(OLD).to_string_lossy().into_owned(),
                    self.paths.bin.join("mluva").to_string_lossy().into_owned(),
                ]
                .iter()
                .any(|value| line == format!("Exec={value}"))
            }) {
                return Err(
                    "The legacy autostart command was customized; it was left untouched.".into(),
                );
            }
        }
        if self.plugin.is_symlink()
            && ![&self.old_data, &self.new_data].iter().any(|root| {
                fs::read_link(&self.plugin)
                    .is_ok_and(|target| target == root.join("app/quickshell/mluva.dictation"))
            })
        {
            return Err(format!(
                "Refusing an unrelated plugin symlink: {}",
                self.plugin.display()
            )
            .into());
        }
        if !self.paths.staged {
            self.desktop.preflight(self.source)?;
        }
        Ok(())
    }

    fn snapshot_paths(&self) -> Vec<PathBuf> {
        let mut paths = vec![
            self.old_data.clone(),
            self.new_data.clone(),
            self.old_config.clone(),
            self.new_config.clone(),
        ];
        paths.extend(
            COMMANDS
                .iter()
                .map(|(name, _)| self.paths.bin.join(name))
                .chain(OLD_COMMANDS.map(|name| self.paths.bin.join(name))),
        );
        paths.extend([OLD_DESKTOP, "com.mluva.Linux"].map(|name| {
            self.paths
                .data
                .join("applications")
                .join(format!("{name}.desktop"))
        }));
        paths.extend([OLD_DESKTOP, "com.mluva.Linux"].map(|name| {
            self.paths
                .data
                .join("icons/hicolor/scalable/apps")
                .join(format!("{name}.svg"))
        }));
        paths.extend(
            [OLD_EXTENSIONS[0], OLD_EXTENSIONS[1], EXTENSION]
                .map(|name| self.extensions.join(name)),
        );
        paths.extend([
            self.secret.join("voice-scribe.env"),
            self.secret.join("mluva.env"),
        ]);
        if present(&self.binding) {
            paths.push(self.binding.clone());
        }
        if present(&self.autostart) {
            paths.extend([
                self.autostart.clone(),
                self.autostart.with_file_name("com.mluva.Linux.desktop"),
            ]);
        }
        if self.plugin.is_symlink() {
            paths.push(self.plugin.clone());
        }
        paths
    }

    fn backup(&self) -> Result<PathBuf> {
        let root = self.paths.data.join("mluva-migration-backups");
        if root.is_symlink() {
            return Err("Refusing a symlinked migration backup directory.".into());
        }
        fs::create_dir_all(&root)?;
        fs::set_permissions(&root, fs::Permissions::from_mode(0o700))?;
        let backup = tempfile::Builder::new()
            .prefix("upgrade-")
            .permissions(fs::Permissions::from_mode(0o700))
            .tempdir_in(root)?;
        let paths = self.snapshot_paths();
        for (index, path) in paths.iter().enumerate() {
            checkpoint()?;
            if present(path) {
                copy(path, &backup.path().join(index.to_string()))?;
            }
        }
        fs::write(
            backup.path().join("manifest.json"),
            serde_json::to_string_pretty(&paths)? + "\n",
        )?;
        fs::write(
            backup.path().join("desktop-settings.json"),
            serde_json::to_string_pretty(&self.desktop.settings)? + "\n",
        )?;
        let backup = backup.keep();
        println!("Mluva migration backup: {}", backup.display());
        Ok(backup)
    }

    fn migrate(&self, tx: &mut Transaction) -> Result<()> {
        for (old, new) in [
            (&self.old_data, &self.new_data),
            (&self.old_config, &self.new_config),
        ] {
            if present(old) {
                tx.relocate(old, new)?;
            }
        }
        state::rebase(tx, &self.old_data, &self.new_data)?;
        for name in OLD_COMMANDS {
            tx.retire(&self.paths.bin.join(name))?;
        }
        for (name, helper) in COMMANDS {
            let path = self.paths.bin.join(name);
            if !present(&path) {
                continue;
            }
            link(
                tx,
                &path,
                &self.paths.app.join(helper.unwrap_or("bin/mluva")),
            )?;
        }
        for suffix in [
            "applications/com.voicescribe.Linux.desktop",
            "icons/hicolor/scalable/apps/com.voicescribe.Linux.svg",
        ] {
            tx.retire(&self.paths.data.join(suffix))?;
        }
        let old_profile = self.secret.join("voice-scribe.env");
        if present(&old_profile) {
            replace(
                tx,
                &self.secret.join("mluva.env"),
                &fs::read(&old_profile)?,
                Some(0o600),
            )?;
            tx.retire(&old_profile)?;
        }
        for name in OLD_EXTENSIONS {
            let path = self.extensions.join(name);
            if !present(&path) {
                continue;
            }
            tx.retire(&path)?;
            if name == OLD_EXTENSIONS[0] {
                let target = self.extensions.join(EXTENSION);
                let expected = Snapshot::read(&target)?;
                copy(
                    &self.source.join("gnome-extension").join(EXTENSION),
                    &tx.prepare(&target)?,
                )?;
                tx.publish(&target, &expected)?;
            }
        }
        if present(&self.binding) {
            let selector = regex::Regex::new(r"^\s*bind\w*\s*=.*\bexec\s*,")?;
            let mut value = String::new();
            for line in text(&self.binding)?.split_inclusive('\n') {
                value.push_str(&if selector.is_match(line) {
                    retarget(line)?
                } else {
                    line.to_owned()
                });
            }
            replace(tx, &self.binding, value.as_bytes(), None)?;
        }
        if present(&self.autostart) {
            let value = text(&self.autostart)?
                .replace(OLD_DESKTOP, "com.mluva.Linux")
                .replace(OLD, "mluva")
                .replace("VoiceScribe", "Mluva")
                .replace("Voice Scribe", "Mluva");
            let mode = fs::metadata(&self.autostart)?.permissions().mode() & 0o7777;
            replace(
                tx,
                &self.autostart.with_file_name("com.mluva.Linux.desktop"),
                value.as_bytes(),
                Some(mode),
            )?;
            tx.retire(&self.autostart)?;
        }
        if self.plugin.is_symlink() {
            link(
                tx,
                &self.plugin,
                &self.paths.app.join("quickshell/mluva.dictation"),
            )?;
        }
        Ok(())
    }
}

fn replace(tx: &mut Transaction, path: &Path, bytes: &[u8], mode: Option<u32>) -> Result<()> {
    let expected = Snapshot::read(path)?;
    let mode = mode.unwrap_or_else(|| {
        fs::metadata(path).map_or(0o644, |meta| meta.permissions().mode() & 0o7777)
    });
    tx.directory(
        path.parent().ok_or("Missing migration parent")?,
        0o755,
        false,
    )?;
    let staged = tx.prepare(path)?;
    fs::write(&staged, bytes)?;
    fs::set_permissions(&staged, fs::Permissions::from_mode(mode))?;
    tx.publish(path, &expected)
}
fn link(tx: &mut Transaction, path: &Path, target: &Path) -> Result<()> {
    let expected = Snapshot::read(path)?;
    symlink(target, tx.prepare(path)?)?;
    tx.publish(path, &expected)
}
fn retarget(value: &str) -> Result<String> {
    Ok(regex::Regex::new(r"(?P<prefix>^|[^\w-])voice-scribe\b")?
        .replace_all(value, "${prefix}mluva")
        .into_owned())
}

pub fn upgrade(
    source: &Path,
    paths: &InstallPaths,
    install: impl FnOnce(&mut Transaction) -> Result<()>,
) -> Result<bool> {
    let mut legacy = Legacy::new(source, paths);
    if !legacy.needed() {
        return Ok(false);
    }
    legacy.preflight()?;
    let backup = legacy.backup()?;
    let mut tx = Transaction::default();
    let result: Result<()> = (|| {
        legacy.migrate(&mut tx)?;
        install(&mut tx)?;
        legacy.desktop.apply()?;
        legacy.desktop.migrate_service(&backup)?;
        checkpoint()?;
        let marker = backup.join("complete");
        fs::write(&marker, [])?;
        fs::set_permissions(marker, fs::Permissions::from_mode(0o600))?;
        tx.commit()?;
        Ok(())
    })();
    if let Err(error) = result {
        let _restoring = Rollback::begin();
        let _ = fs::remove_file(backup.join("complete"));
        legacy.desktop.restore_service(&backup);
        drop(tx);
        legacy.desktop.restore();
        eprintln!(
            "Mluva upgrade rolled back; backup retained at {}.",
            backup.display()
        );
        return Err(error);
    }
    println!(
        "Mluva identity migration completed. Desktop permissions may need approval under the new identity."
    );
    Ok(true)
}
