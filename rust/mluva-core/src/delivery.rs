//! Clipboard-first delivery with one insertion attempt and an honest durable receipt.
use crate::executables::find_executable;
use caseless::default_case_fold_str as casefold;
use std::{
    collections::HashMap,
    error::Error,
    ffi::OsString,
    fs,
    io::{self, Write},
    os::unix::{fs::MetadataExt, net::UnixDatagram},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct DeliveryReceipt {
    pub copied: bool,
    pub pasted: bool,
    pub guidance: String,
    pub paste_dispatched: bool,
    pub paste_confirmed: Option<bool>,
}

impl DeliveryReceipt {
    pub fn history_outcome(&self) -> &'static str {
        if self.pasted {
            "pasted"
        } else if self.paste_dispatched {
            "paste-unconfirmed"
        } else if self.copied {
            "copied"
        } else {
            "ready"
        }
    }

    fn copied(guidance: &str) -> Self {
        Self {
            copied: true,
            pasted: false,
            guidance: guidance.into(),
            paste_dispatched: false,
            paste_confirmed: None,
        }
    }

    fn attempted(guidance: &str, confirmation: Option<bool>) -> Self {
        Self {
            pasted: confirmation == Some(true),
            paste_dispatched: true,
            paste_confirmed: confirmation,
            ..Self::copied(guidance)
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum DeliveryError {
    #[error("Cannot deliver empty text.")]
    EmptyText,
    #[error("Paste confirmation timeout cannot be negative.")]
    NegativeTimeout,
    #[error("Install wl-clipboard on Wayland or xclip on X11.")]
    MissingClipboard,
    #[error("Could not copy text to the desktop clipboard: {0}")]
    Clipboard(#[source] io::Error),
}

/// Target failures after an attempt become uncertainty, never permission for a second insertion.
pub type TargetResult<T> = Result<T, Box<dyn Error>>;
pub type ConfirmPaste<'a> = &'a mut dyn FnMut() -> TargetResult<Option<bool>>;
pub type InsertDirectly<'a> = &'a mut dyn FnMut(&str) -> TargetResult<Option<bool>>;
pub type AuthorizeKeyboardPaste<'a> = &'a mut dyn FnMut() -> TargetResult<bool>;

pub struct DeliveryOptions<'a> {
    pub confirm_paste: Option<ConfirmPaste<'a>>,
    pub insert_directly: Option<InsertDirectly<'a>>,
    pub authorize_keyboard_paste: Option<AuthorizeKeyboardPaste<'a>>,
    pub confirmation_timeout_seconds: f64,
    pub application_identifier: Option<&'a str>,
}

impl Default for DeliveryOptions<'_> {
    fn default() -> Self {
        Self {
            confirm_paste: None,
            insert_directly: None,
            authorize_keyboard_paste: None,
            confirmation_timeout_seconds: 0.75,
            application_identifier: None,
        }
    }
}

/// Run on the capture worker, with callbacks bound to the originally captured target.
pub fn deliver_text(
    text: &str,
    auto_paste: bool,
    mut options: DeliveryOptions<'_>,
) -> Result<DeliveryReceipt, DeliveryError> {
    if text.is_empty() {
        return Err(DeliveryError::EmptyText);
    }
    if options.confirmation_timeout_seconds < 0.0 {
        return Err(DeliveryError::NegativeTimeout);
    }
    let environment = std::env::vars_os().collect();
    let clipboard = find_executable(if x11(&environment) {
        "xclip"
    } else {
        "wl-copy"
    })
    .ok_or(DeliveryError::MissingClipboard)?;
    let arguments = if clipboard
        .as_os_str()
        .as_encoded_bytes()
        .ends_with(b"wl-copy")
    {
        &[][..]
    } else {
        &["-selection", "clipboard"][..]
    };
    copy(&clipboard, arguments, text).map_err(DeliveryError::Clipboard)?;
    if !auto_paste {
        return Ok(DeliveryReceipt::copied(
            "Copied. Paste in the target application.",
        ));
    }
    if let Some(insert) = options.insert_directly.as_mut() {
        match insert(text).unwrap_or(Some(false)) {
            Some(true) => {
                return Ok(DeliveryReceipt::attempted(
                    "Inserted once into the restored target through its accessibility editing interface.",
                    Some(true),
                ));
            }
            Some(false) => {
                return Ok(DeliveryReceipt::attempted(
                    "Automatic insertion was attempted once, but the captured target did not confirm it. The complete text remains on the clipboard; inspect the target before any manual retry.",
                    Some(false),
                ));
            }
            None => {}
        }
    }
    let Some(command) = paste_command(&environment, options.application_identifier) else {
        return Ok(DeliveryReceipt::copied(
            "Copied. No layout-safe keyboard paste helper is ready for this target; paste manually.",
        ));
    };
    thread::sleep(Duration::from_millis(120));
    if options
        .authorize_keyboard_paste
        .as_mut()
        .is_some_and(|authorize| !authorize().unwrap_or(false))
    {
        return Ok(DeliveryReceipt::copied(
            "The captured target could not be revalidated immediately before keyboard delivery, so no paste was attempted. The complete text remains on the clipboard.",
        ));
    }
    if !Command::new(&command[0])
        .args(&command[1..])
        .status()
        .is_ok_and(|status| status.success())
    {
        return Ok(DeliveryReceipt::attempted(
            "Paste was attempted once, but the input injector did not report success. The complete text remains on the clipboard; inspect the target before any manual retry.",
            None,
        ));
    }
    let confirmation = confirm_paste(options.confirm_paste, options.confirmation_timeout_seconds);
    let guidance = match confirmation {
        Some(true) => "Inserted once into the restored target application.",
        Some(false) => {
            "Paste was sent once, but the captured target did not confirm insertion. The complete text remains on the clipboard; inspect the target before any manual retry."
        }
        None => {
            "Paste was sent once, but this target could not confirm insertion. The complete text remains on the clipboard; inspect the target before any manual retry."
        }
    };
    Ok(DeliveryReceipt::attempted(guidance, confirmation))
}

fn copy(executable: &Path, arguments: &[&str], text: &str) -> io::Result<()> {
    let mut child = Command::new(executable)
        .args(arguments)
        .stdin(Stdio::piped())
        .spawn()?;
    let written = child.stdin.take().unwrap().write_all(text.as_bytes());
    let status = child.wait()?;
    if !status.success() {
        return Err(io::Error::other(format!(
            "clipboard helper exited with {status}"
        )));
    }
    // Successful clipboard helpers may close the pipe themselves; subprocess.run accepts that acknowledgement.
    match written {
        Err(error) if error.kind() == io::ErrorKind::BrokenPipe => Ok(()),
        result => result,
    }
}

fn confirm_paste(mut callback: Option<ConfirmPaste<'_>>, timeout: f64) -> Option<bool> {
    let callback = callback.as_mut()?;
    let started = Instant::now();
    loop {
        let confirmation = callback().unwrap_or(None);
        if confirmation != Some(false) {
            return confirmation;
        }
        let remaining = timeout - started.elapsed().as_secs_f64();
        if remaining <= 0.0 {
            return Some(false);
        }
        thread::sleep(Duration::from_secs_f64(0.05_f64.min(remaining)));
    }
}

/// Check installed helpers and a live private daemon without emitting an input event.
pub fn keyboard_paste_available(
    environment: Option<&HashMap<OsString, OsString>>,
    application_identifier: Option<&str>,
) -> bool {
    let inherited;
    let environment = match environment {
        Some(environment) => environment,
        None => {
            inherited = std::env::vars_os().collect();
            &inherited
        }
    };
    paste_command(environment, application_identifier).is_some()
}

fn paste_command(
    environment: &HashMap<OsString, OsString>,
    application_identifier: Option<&str>,
) -> Option<Vec<OsString>> {
    let terminal = terminal_identifier(application_identifier.unwrap_or(""));
    let arguments: &[&str];
    let executable;
    if !x11(environment)
        && let Some(path) = find_executable("wtype")
    {
        executable = path;
        arguments = if terminal {
            &[
                "-M", "ctrl", "-M", "shift", "v", "-m", "shift", "-m", "ctrl",
            ]
        } else {
            &["-M", "ctrl", "v", "-m", "ctrl"]
        };
    } else if x11(environment)
        && let Some(path) = find_executable("xdotool")
    {
        executable = path;
        arguments = if terminal {
            &["key", "--clearmodifiers", "ctrl+shift+v"]
        } else {
            &["key", "--clearmodifiers", "ctrl+v"]
        };
    } else if terminal
        && let Some(path) = find_executable("ydotool")
        && ydotool_socket_ready(environment)
    {
        executable = path;
        arguments = &["key", "42:1", "110:1", "110:0", "42:0"];
    } else {
        return None;
    }
    Some(
        std::iter::once(executable.into_os_string())
            .chain(arguments.iter().map(OsString::from))
            .collect(),
    )
}

pub(crate) fn terminal_identifier(identifier: &str) -> bool {
    let identifier = identifier.strip_prefix("process:").unwrap_or(identifier);
    let name = casefold(
        Path::new(identifier)
            .file_name()
            .and_then(|name| name.to_str())
            .unwrap_or(""),
    );
    matches!(
        name.as_str(),
        "alacritty" | "foot" | "footclient" | "ghostty" | "kitty" | "wezterm" | "wezterm-gui"
    )
}

fn x11(environment: &HashMap<OsString, OsString>) -> bool {
    let session = environment
        .get(std::ffi::OsStr::new("XDG_SESSION_TYPE"))
        .map(|value| casefold(&value.to_string_lossy()))
        .unwrap_or_default();
    if !session.is_empty() {
        return session == "x11";
    }
    !variable(environment, "DISPLAY").is_empty()
        && variable(environment, "WAYLAND_DISPLAY").is_empty()
}

fn variable<'a>(environment: &'a HashMap<OsString, OsString>, name: &str) -> &'a std::ffi::OsStr {
    environment
        .get(std::ffi::OsStr::new(name))
        .map(OsString::as_os_str)
        .unwrap_or_default()
}

fn ydotool_socket_ready(environment: &HashMap<OsString, OsString>) -> bool {
    let configured = variable(environment, "YDOTOOL_SOCKET");
    let runtime = variable(environment, "XDG_RUNTIME_DIR");
    let path = if !configured.is_empty() {
        PathBuf::from(configured)
    } else if !runtime.is_empty() {
        PathBuf::from(runtime).join(".ydotool_socket")
    } else {
        PathBuf::from("/tmp/.ydotool_socket")
    };
    let Ok(metadata) = fs::metadata(&path) else {
        return false;
    };
    if metadata.mode() & libc::S_IFMT != libc::S_IFSOCK
        || metadata.uid() != unsafe { libc::geteuid() }
        || metadata.mode() & 0o077 != 0
        || metadata.mode() & 0o600 != 0o600
    {
        return false;
    }
    UnixDatagram::unbound().is_ok_and(|client| {
        client
            .set_write_timeout(Some(Duration::from_millis(100)))
            .is_ok()
            && client.connect(&path).is_ok()
    })
}
