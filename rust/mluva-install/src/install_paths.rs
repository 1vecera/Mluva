//! Resolve the user prefix once, before any installation or removal effects.
use std::{
    env, fs,
    os::unix::ffi::OsStrExt,
    path::{Path, PathBuf},
};

pub type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

pub struct InstallPaths {
    pub data: PathBuf,
    pub config: PathBuf,
    pub bin: PathBuf,
    pub app: PathBuf,
    pub staged: bool,
}

fn variable(name: &str) -> Option<PathBuf> {
    env::var_os(name)
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
}

fn resolved(path: &Path) -> Result<PathBuf> {
    let mut ancestor = path.to_owned();
    let mut suffix = Vec::new();
    loop {
        match ancestor.canonicalize() {
            Ok(mut result) => {
                for name in suffix.into_iter().rev() {
                    result.push(name);
                }
                return Ok(result);
            }
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
                suffix.push(
                    ancestor
                        .file_name()
                        .ok_or("An installation root cannot be resolved.")?
                        .to_owned(),
                );
                if !ancestor.pop() {
                    return Err(error.into());
                }
            }
            Err(error) => return Err(error.into()),
        }
    }
}

impl InstallPaths {
    pub fn from_environment(action: &str) -> Result<Self> {
        let live_home = variable("HOME").ok_or("HOME must name the desktop user's directory.")?;
        let home = variable("MLUVA_INSTALL_HOME").unwrap_or_else(|| live_home.clone());
        if !home.is_absolute() || home == Path::new("/") {
            return Err("MLUVA_INSTALL_HOME must be an absolute non-root directory.".into());
        }
        let staged = home != live_home;
        let data = if staged {
            None
        } else {
            variable("XDG_DATA_HOME")
        }
        .unwrap_or_else(|| home.join(".local/share"));
        let config = if staged {
            None
        } else {
            variable("XDG_CONFIG_HOME")
        }
        .unwrap_or_else(|| home.join(".config"));
        for root in [&data, &config] {
            if !root.is_absolute() || root == Path::new("/") {
                return Err(format!(
                    "Refusing to {action} through an unsafe XDG root: {}",
                    root.display()
                )
                .into());
            }
        }
        let bin = home.join(".local/bin");
        if staged {
            let root = resolved(&home)?;
            if root == resolved(&live_home)? || root == Path::new("/") {
                return Err("The staged installation resolves to the live home directory.".into());
            }
            for directory in [&data, &config, &bin] {
                if !resolved(directory)?.starts_with(&root) {
                    return Err(format!(
                        "A staged directory resolves outside the installation home: {}",
                        directory.display()
                    )
                    .into());
                }
            }
        }
        Ok(Self {
            app: data.join("mluva/app"),
            data,
            config,
            bin,
            staged,
        })
    }
}

pub fn regular(path: &Path) -> bool {
    path.symlink_metadata()
        .is_ok_and(|metadata| metadata.is_file())
}

pub fn native_owner(app: &Path) -> bool {
    let marker = app.join(".mluva-native.json");
    regular(&marker)
        && fs::read(marker)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
            .is_some_and(|value| {
                value["schema"] == 1
                    && value["application"] == "com.mluva.Linux"
                    && value["implementation"] == "rust"
            })
}

pub fn running(app: &Path) -> Result<bool> {
    let canonical = resolved(app)?;
    let legacy = app.join(".venv/bin/python");
    for entry in fs::read_dir("/proc")? {
        let entry = entry?;
        let Some(pid) = entry
            .file_name()
            .to_str()
            .and_then(|name| name.parse::<u32>().ok())
        else {
            continue;
        };
        if pid == std::process::id() {
            continue;
        }
        if fs::read_link(entry.path().join("exe"))
            .is_ok_and(|executable| executable.starts_with(&canonical))
        {
            return Ok(true);
        }
        // Inspect only invocation identity, never other processes' environment
        // or open file contents. Python's executable also lives outside its venv.
        if let Ok(bytes) = fs::read(entry.path().join("cmdline")) {
            let args: Vec<_> = bytes.split(|byte| *byte == 0).collect();
            // Executable links can be unreadable across a user namespace or
            // for a nondumpable process. Resolve an absolute invocation too,
            // including the managed public symlink, before allowing removal.
            let invoked = Path::new(std::ffi::OsStr::from_bytes(args[0]));
            if invoked.is_absolute()
                && resolved(invoked).is_ok_and(|program| program.starts_with(&canonical))
            {
                return Ok(true);
            }
            if args.len() >= 3
                && args[0] == legacy.as_os_str().as_encoded_bytes()
                && args[1] == b"-m"
                && args[2] == b"mluva_linux.app"
            {
                return Ok(true);
            }
        }
    }
    Ok(false)
}
