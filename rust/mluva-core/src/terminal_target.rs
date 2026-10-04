//! Content-free Hyprland terminal identity, revalidated without activating any window.
use crate::{delivery::terminal_identifier, executables::find_executable, text};
use serde_json::Value;
use std::{
    collections::HashMap,
    ffi::{OsStr, OsString},
    fs,
    io::{self, Read},
    os::fd::AsRawFd,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    time::{Duration, Instant},
};

#[derive(Clone)]
pub struct TerminalTargetSnapshot {
    address: String,
    process_id: u32,
    application_identifier: String,
    hyprctl: PathBuf,
    environment: HashMap<OsString, OsString>,
}

impl std::fmt::Debug for TerminalTargetSnapshot {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter
            .debug_struct("TerminalTargetSnapshot")
            .field("address", &self.address)
            .field("process_id", &self.process_id)
            .field("application_identifier", &self.application_identifier)
            .field("hyprctl", &self.hyprctl)
            .finish()
    }
}

impl TerminalTargetSnapshot {
    pub fn address(&self) -> &str {
        &self.address
    }
    pub fn process_id(&self) -> u32 {
        self.process_id
    }
    pub fn application_identifier(&self) -> &str {
        &self.application_identifier
    }

    /// Authorize only the same currently focused address, process and executable.
    pub fn restore(&self) -> bool {
        focused_terminal(&self.hyprctl, &self.environment).is_some_and(|identity| {
            identity
                == (
                    self.address.clone(),
                    self.process_id,
                    self.application_identifier.clone(),
                )
        })
    }

    /// A terminal offers neither a content-free editing interface nor an observable text caret.
    pub fn insert_text(&self, _text: &str) -> Option<bool> {
        None
    }
    pub fn confirm_insertion(&self, _text: &str) -> Option<bool> {
        None
    }
}

pub fn hyprland_terminal_tracking_available(
    environment: Option<&HashMap<OsString, OsString>>,
) -> bool {
    let signature = match environment {
        Some(environment) => environment
            .get(OsStr::new("HYPRLAND_INSTANCE_SIGNATURE"))
            .cloned(),
        None => std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE"),
    };
    signature.is_some_and(|signature| !signature.is_empty()) && find_executable("hyprctl").is_some()
}

pub fn capture_hyprland_terminal_target() -> Option<TerminalTargetSnapshot> {
    if !hyprland_terminal_tracking_available(None) {
        return None;
    }
    let hyprctl = find_executable("hyprctl")?;
    let environment = ["HYPRLAND_INSTANCE_SIGNATURE", "XDG_RUNTIME_DIR"]
        .into_iter()
        .filter_map(|key| std::env::var_os(key).map(|value| (OsString::from(key), value)))
        .collect();
    let (address, process_id, application_identifier) = focused_terminal(&hyprctl, &environment)?;
    Some(TerminalTargetSnapshot {
        address,
        process_id,
        application_identifier,
        hyprctl,
        environment,
    })
}

fn focused_terminal(
    executable: &Path,
    environment: &HashMap<OsString, OsString>,
) -> Option<(String, u32, String)> {
    let response = query(executable, environment).ok()?;
    let window: Value = serde_json::from_slice(&non_finite_tokens(&response)).ok()?;
    let address = window["address"].as_str()?;
    if !nonzero_hex_address(address) {
        return None;
    }
    let pid = u32::try_from(window["pid"].as_u64()?).ok()?;
    if pid == 0 || !truthy(&window["mapped"]) {
        return None;
    }
    let path = fs::read_link(Path::new("/proc").join(pid.to_string()).join("exe")).ok()?;
    let path = path.to_str()?;
    let executable = path.strip_suffix(" (deleted)").unwrap_or(path);
    if !terminal_identifier(executable) {
        return None;
    }
    Some((address.into(), pid, executable.into()))
}

fn nonzero_hex_address(address: &str) -> bool {
    let Some(digits) = address.strip_prefix("0x") else {
        return false;
    };
    let digits = digits.trim_end_matches(|character: char| {
        character.is_ascii_whitespace() || (!character.is_ascii() && text::whitespace(character))
    });
    let digits = digits.strip_prefix('_').unwrap_or(digits);
    let mut previous_digit = false;
    let mut nonzero = false;
    for character in digits.chars() {
        if character == '_' && previous_digit {
            previous_digit = false;
        } else if let Some(value) = character
            .to_digit(16)
            .or_else(|| text::decimal_value(character))
        {
            previous_digit = true;
            nonzero |= value != 0;
        } else {
            return false;
        }
    }
    previous_digit && nonzero
}

/// These fields only inspect number type and truthiness; nonfinite values stay noninteger and truthy.
fn non_finite_tokens(bytes: &[u8]) -> Vec<u8> {
    let mut output = Vec::with_capacity(bytes.len());
    let mut quoted = false;
    let mut escaped = false;
    let mut index = 0;
    let mut prefix = None;
    while index < bytes.len() {
        let byte = bytes[index];
        if quoted {
            output.push(byte);
            if escaped {
                escaped = false;
            } else if byte == b'\\' {
                escaped = true;
            } else if byte == b'"' {
                quoted = false;
            }
            index += 1;
            continue;
        }
        if byte == b'"' {
            quoted = true;
        }
        let token = [b"-Infinity".as_slice(), b"Infinity", b"NaN"]
            .into_iter()
            .find(|token| {
                (prefix.is_none() || matches!(prefix, Some(b':' | b',' | b'[')))
                    && bytes[index..].starts_with(token)
                    && bytes.get(index + token.len()).is_none_or(|byte| {
                        byte.is_ascii_whitespace() || matches!(*byte, b',' | b'}' | b']')
                    })
            });
        if let Some(token) = token {
            output.extend_from_slice(b"1e9999");
            index += token.len();
            prefix = Some(b'9');
        } else {
            output.push(byte);
            if !byte.is_ascii_whitespace() {
                prefix = Some(byte);
            }
            index += 1;
        }
    }
    output
}

fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64() != Some(0.0),
        Value::String(value) => !value.is_empty(),
        Value::Array(value) => !value.is_empty(),
        Value::Object(value) => !value.is_empty(),
    }
}

/// Drain both native pipes under the released half-second deadline and always reap the owned query.
fn query(executable: &Path, environment: &HashMap<OsString, OsString>) -> io::Result<Vec<u8>> {
    let mut child = Command::new(executable)
        .args(["-j", "activewindow"])
        .env_clear()
        .envs(environment)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()?;
    let deadline = Instant::now() + Duration::from_millis(500);
    let mut stdout = child.stdout.take().unwrap();
    let mut stderr = child.stderr.take().unwrap();
    let result = (|| {
        let mut fds = [stdout.as_raw_fd(), stderr.as_raw_fd()].map(|fd| libc::pollfd {
            fd,
            events: libc::POLLIN,
            revents: 0,
        });
        for fd in &fds {
            let flags = unsafe { libc::fcntl(fd.fd, libc::F_GETFL) };
            if flags == -1
                || unsafe { libc::fcntl(fd.fd, libc::F_SETFL, flags | libc::O_NONBLOCK) } == -1
            {
                return Err(io::Error::last_os_error());
            }
        }
        let mut response = vec![];
        loop {
            let remaining = deadline
                .checked_duration_since(Instant::now())
                .ok_or_else(|| {
                    io::Error::new(io::ErrorKind::TimedOut, "terminal query timed out")
                })?;
            if fds.iter().all(|fd| fd.fd == -1) {
                if let Some(status) = child.try_wait()? {
                    return if status.success() {
                        Ok(response)
                    } else {
                        Err(io::Error::other("terminal query failed"))
                    };
                }
                std::thread::sleep(remaining.min(Duration::from_millis(5)));
                continue;
            }
            let polled = unsafe {
                libc::poll(
                    fds.as_mut_ptr(),
                    fds.len() as libc::nfds_t,
                    remaining.as_millis().min(i32::MAX as u128) as i32,
                )
            };
            if polled < 0 {
                let error = io::Error::last_os_error();
                if error.kind() == io::ErrorKind::Interrupted {
                    continue;
                }
                return Err(error);
            }
            for (index, fd) in fds.iter_mut().enumerate() {
                if fd.fd == -1 || fd.revents == 0 {
                    continue;
                }
                let stream: &mut dyn Read = if index == 0 { &mut stdout } else { &mut stderr };
                let mut buffer = [0; 8192];
                loop {
                    if Instant::now() >= deadline {
                        return Err(io::Error::new(
                            io::ErrorKind::TimedOut,
                            "terminal query timed out",
                        ));
                    }
                    match stream.read(&mut buffer) {
                        Ok(0) => {
                            fd.fd = -1;
                            break;
                        }
                        Ok(count) => {
                            if index == 0 {
                                response.extend_from_slice(&buffer[..count]);
                            }
                        }
                        Err(error) if error.kind() == io::ErrorKind::WouldBlock => break,
                        Err(error) if error.kind() == io::ErrorKind::Interrupted => continue,
                        Err(error) => return Err(error),
                    }
                }
            }
        }
    })();
    if child.try_wait()?.is_none() {
        let _ = child.kill();
    }
    child.wait()?;
    result
}
