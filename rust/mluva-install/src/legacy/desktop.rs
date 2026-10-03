//! Migrate existing GNOME preferences and the exact same-user input service.
//! No new permission, overlay enablement, or other user's service is introduced.
use super::{
    EXTENSION, OLD_DESKTOP, OLD_EXTENSIONS, Result, copy, present, require_regular, retarget,
};
use crate::install_process::{Failed, authenticate, output, run};
use std::{fs, os::unix::fs::MetadataExt, path::Path, process::Command};

pub(super) const OLD_UNIT: &str = "/etc/systemd/system/voice-scribe-input@.service";
const NEW_UNIT: &str = "/etc/systemd/system/mluva-input@.service";

#[derive(Default)]
pub(super) struct Desktop {
    pub settings: Vec<(String, String, String)>,
    service: bool,
    service_touched: bool,
    unit_identity: Option<(u64, u64)>,
    enabled: bool,
    active: bool,
    expected_unit: Vec<u8>,
}

impl Desktop {
    pub fn preflight(&mut self, source: &Path) -> Result<()> {
        if crate::available("gsettings") {
            for key in ["favorite-apps", "enabled-extensions", "disabled-extensions"] {
                let (code, value) =
                    output(Command::new("gsettings").args(["get", "org.gnome.shell", key]))?;
                if code == 0 {
                    self.settings
                        .push(("org.gnome.shell".into(), key.into(), value.trim().into()));
                }
            }
            let (code, value) = output(Command::new("gsettings").args([
                "get",
                "org.gnome.settings-daemon.plugins.media-keys",
                "custom-keybindings",
            ]))?;
            if code == 0 {
                let paths = glib::Variant::parse(None, value.trim())
                    .ok()
                    .and_then(|value| value.get::<Vec<String>>())
                    .ok_or(
                        "The custom-keybinding catalog is invalid; settings were left unchanged.",
                    )?;
                for path in paths {
                    let schema = format!(
                        "org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:{path}"
                    );
                    let (code, value) =
                        output(Command::new("gsettings").args(["get", &schema, "command"]))?;
                    if code == 0 && value.contains("voice-scribe") {
                        self.settings
                            .push((schema, "command".into(), value.trim().into()));
                    }
                }
            }
        }
        let unit = Path::new(OLD_UNIT);
        if present(unit) {
            require_regular(unit)?;
            self.expected_unit = fs::read(source.join("resources/mluva-input@.service"))?;
            if fs::read(unit)? != self.expected_unit {
                return Err(
                    "The legacy system input service has local changes; it was left untouched."
                        .into(),
                );
            }
            if present(Path::new(NEW_UNIT)) {
                return Err(
                    "Both input service templates exist; reconcile them before upgrading.".into(),
                );
            }
            let (code, value) = output(Command::new("systemctl").args([
                "list-units",
                "--all",
                "--plain",
                "--no-legend",
                "voice-scribe-input@*.service",
            ]))?;
            if code != 0 {
                return Err(Failed(code).into());
            }
            let own = format!("voice-scribe-input@{}.service", unsafe { libc::getuid() });
            let pattern = regex::Regex::new(r"voice-scribe-input@\S+\.service")?;
            let mut instances: Vec<String> = pattern
                .find_iter(&value)
                .map(|value| value.as_str().to_owned())
                .collect();
            for entry in fs::read_dir(unit.parent().ok_or("Missing unit parent")?)? {
                let entry = entry?;
                if entry.file_name().to_string_lossy().ends_with(".wants") && entry.path().is_dir()
                {
                    for link in fs::read_dir(entry.path())? {
                        let name = link?.file_name().to_string_lossy().into_owned();
                        if name.starts_with("voice-scribe-input@") && name.ends_with(".service") {
                            instances.push(name);
                        }
                    }
                }
            }
            if instances.iter().any(|name| name != &own) {
                return Err("The legacy input template has other users' instances; migrate those together first.".into());
            }
            authenticate(Command::new("sudo").arg("-v"))?;
            self.service = true;
        }
        Ok(())
    }

    pub fn apply(&self) -> Result<()> {
        let obsolete = regex::Regex::new(r"'right-alt@voicescribe\.local',?\s*")?;
        for (schema, key, old) in &self.settings {
            let mut value = old
                .replace(OLD_DESKTOP, "com.mluva.Linux")
                .replace(OLD_EXTENSIONS[0], EXTENSION);
            if key == "command" {
                value = retarget(&value)?;
            }
            let value = obsolete.replace_all(&value, "").replace(", ]", "]");
            run(Command::new("gsettings").args(["set", schema, key, &value]))?;
        }
        Ok(())
    }
    pub fn restore(&self) {
        for (schema, key, value) in &self.settings {
            if run(Command::new("gsettings").args(["set", schema, key, value])).is_err() {
                eprintln!(
                    "A desktop preference could not be restored; its original value is in the private migration backup."
                );
            }
        }
    }

    pub fn migrate_service(&mut self, backup: &Path) -> Result<()> {
        if !self.service {
            return Ok(());
        }
        let uid = unsafe { libc::getuid() };
        let old = format!("voice-scribe-input@{uid}.service");
        let new = format!("mluva-input@{uid}.service");
        self.enabled =
            output(Command::new("systemctl").args(["is-enabled", "--quiet", &old]))?.0 == 0;
        self.active =
            output(Command::new("systemctl").args(["is-active", "--quiet", &old]))?.0 == 0;
        if present(Path::new(NEW_UNIT))
            || !super::regular(Path::new(OLD_UNIT))
            || fs::read(OLD_UNIT)? != self.expected_unit
        {
            return Err(
                "The input service changed during migration; it was left untouched.".into(),
            );
        }
        copy(Path::new(OLD_UNIT), &backup.join("input-service"))?;
        let metadata = fs::symlink_metadata(OLD_UNIT)?;
        self.unit_identity = Some((metadata.dev(), metadata.ino()));
        self.service_touched = true;
        run(Command::new("sudo").args(["systemctl", "disable", "--now", &old]))?;
        // Never clobber a template that appears after preflight.
        run(Command::new("sudo").args([
            "mv",
            "--no-clobber",
            "--no-target-directory",
            OLD_UNIT,
            NEW_UNIT,
        ]))?;
        if present(Path::new(OLD_UNIT)) || !self.owns_unit(Path::new(NEW_UNIT)) {
            return Err(
                "The input template could not be moved without replacing another owner.".into(),
            );
        }
        run(Command::new("sudo").args(["systemctl", "daemon-reload"]))?;
        if self.enabled {
            run(Command::new("sudo").args(["systemctl", "enable", &new]))?;
        }
        if self.active {
            run(Command::new("sudo").args(["systemctl", "start", &new]))?;
        }
        Ok(())
    }

    fn owns_unit(&self, path: &Path) -> bool {
        path.symlink_metadata().is_ok_and(|meta| {
            meta.is_file() && self.unit_identity == Some((meta.dev(), meta.ino()))
        }) && fs::read(path).is_ok_and(|bytes| bytes == self.expected_unit)
    }

    pub fn restore_service(&mut self, backup: &Path) {
        if !self.service_touched {
            return;
        }
        let result = (|| -> Result<()> {
            let uid = unsafe { libc::getuid() };
            let old = format!("voice-scribe-input@{uid}.service");
            let new = format!("mluva-input@{uid}.service");
            // A successful rename may be followed by cancellation before the
            // command returns. Its original inode proves ownership even then.
            if !present(Path::new(OLD_UNIT)) {
                if !self.owns_unit(Path::new(NEW_UNIT)) {
                    return Err("The new input service changed".into());
                }
                run(Command::new("sudo").args(["systemctl", "disable", "--now", &new]))?;
                run(Command::new("sudo").args(["rm", "-f", NEW_UNIT]))?;
            } else if !self.owns_unit(Path::new(OLD_UNIT)) {
                return Err("The old input service changed".into());
            }
            run(Command::new("sudo")
                .args(["install", "-m", "0644"])
                .arg(backup.join("input-service"))
                .arg(OLD_UNIT))?;
            run(Command::new("sudo").args(["systemctl", "daemon-reload"]))?;
            if self.enabled {
                run(Command::new("sudo").args(["systemctl", "enable", &old]))?;
            }
            if self.active {
                run(Command::new("sudo").args(["systemctl", "start", &old]))?;
            }
            Ok(())
        })();
        if result.is_err() {
            eprintln!(
                "The input service could not be fully restored. Its template remains at {}.",
                backup.join("input-service").display()
            );
        }
        self.service_touched = false;
    }
}
