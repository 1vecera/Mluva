//! What differs between operating systems in reaching agent sessions: process identity, where
//! Claude Code's inboxes live, who holds a Codex thread's lock, which executable runs Codex and
//! how a link is opened.
//!
//! Linux is implemented and tested. macOS branches compile but answer "unavailable" until each
//! is verified on a Mac; they say what the Mac needs instead. Callers treat "unavailable" as no
//! route, so an unverified platform shows its sessions as draft only rather than guessing.
use std::{
    collections::HashMap,
    ffi::OsString,
    path::{Path, PathBuf},
    time::Duration,
};
use tokio::process::Command;

/// A process's start time, as the kernel counts it: field 22 of `/proc/<pid>/stat` on Linux.
/// Claude Code records the same value as `procStart` to detect PID reuse.
#[cfg(target_os = "linux")]
pub fn process_start(pid: u32) -> Option<String> {
    stat_fields(pid)?.into_iter().nth(19)
}

/// macOS needs `proc_pidinfo(PROC_PIDTBSDINFO)` and the format Claude Code records there, which
/// has not been verified.
#[cfg(not(target_os = "linux"))]
pub fn process_start(_pid: u32) -> Option<String> {
    None
}

/// A process's parent.
#[cfg(target_os = "linux")]
pub fn parent(pid: u32) -> Option<u32> {
    stat_fields(pid)?.get(1)?.parse().ok()
}

/// macOS needs `proc_pidinfo(PROC_PIDTBSDINFO).pbi_ppid`; unverified.
#[cfg(not(target_os = "linux"))]
pub fn parent(_pid: u32) -> Option<u32> {
    None
}

/// The fields of `/proc/<pid>/stat` after the parenthesised command name, from `state`.
#[cfg(target_os = "linux")]
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

/// This process's PID namespace, which a Claude Code registry entry must share. macOS has no
/// PID namespaces; an entry's domain is then not compared.
pub fn own_namespace() -> Option<String> {
    std::fs::read_link("/proc/self/ns/pid")
        .ok()
        .map(|link| link.to_string_lossy().into_owned())
}

/// The effective user, the only owner Mluva trusts for sockets and files.
pub fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments or memory preconditions.
    unsafe { libc::geteuid() }
}

/// The directories Claude Code binds its inboxes in. Linux: `$XDG_RUNTIME_DIR/cc-socks`, and
/// `/tmp/cc-socks-<uid>` when the runtime directory is unusable. macOS: `/tmp/cc-socks`.
pub fn inbox_directories() -> Vec<PathBuf> {
    let uid = effective_uid();
    if cfg!(target_os = "macos") {
        // `/tmp` is a symlink to `/private/tmp`; the registry may name either.
        return vec![
            PathBuf::from("/tmp/cc-socks"),
            PathBuf::from("/private/tmp/cc-socks"),
        ];
    }
    let mut directories: Vec<PathBuf> = std::env::var_os("XDG_RUNTIME_DIR")
        .filter(|dir| !dir.is_empty())
        .map(|dir| PathBuf::from(dir).join("cc-socks"))
        .into_iter()
        .collect();
    directories.push(PathBuf::from(format!("/tmp/cc-socks-{uid}")));
    directories
}

/// The connected peer's user and process. Linux uses `SO_PEERCRED`; on macOS tokio reads
/// `getpeereid` and `LOCAL_PEERPID`, which has not been verified against Claude Code.
pub fn peer_process(stream: &tokio::net::UnixStream) -> Option<(u32, u32)> {
    let peer = stream.peer_cred().ok()?;
    Some((peer.uid(), u32::try_from(peer.pid()?).ok()?))
}

/// Holders of exclusive `flock` locks, by the locked file's device and inode, from
/// `/proc/locks` on Linux. A lock file left behind by a process that exited has no holder.
#[cfg(target_os = "linux")]
pub fn flock_holders() -> HashMap<(u64, u64), u32> {
    std::fs::read_to_string("/proc/locks")
        .map(|locks| parse_flock_holders(&locks))
        .unwrap_or_default()
}

/// macOS has no `/proc/locks`; finding a lock's holder there needs another method, such as
/// `lsof`, which has not been verified.
#[cfg(not(target_os = "linux"))]
pub fn flock_holders() -> HashMap<(u64, u64), u32> {
    HashMap::new()
}

/// Parse `/proc/locks` lines such as `15: FLOCK  ADVISORY  WRITE 954587 00:1e:39635243 0 EOF`
/// into (device, inode) → PID, for exclusive `flock` locks only. Blocked waiters (`->`) are not
/// holders.
pub(crate) fn parse_flock_holders(locks: &str) -> HashMap<(u64, u64), u32> {
    let mut holders = HashMap::new();
    for line in locks.lines().take(65_536) {
        let fields: Vec<&str> = line.split_whitespace().collect();
        let [_, kind, _, mode, pid, file, ..] = fields.as_slice() else {
            continue;
        };
        if *kind != "FLOCK" || *mode != "WRITE" {
            continue;
        }
        let mut parts = file.split(':');
        let (Some(major), Some(minor), Some(inode), None) =
            (parts.next(), parts.next(), parts.next(), parts.next())
        else {
            continue;
        };
        let (Ok(major), Ok(minor), Ok(inode), Ok(pid)) = (
            u32::from_str_radix(major, 16),
            u32::from_str_radix(minor, 16),
            inode.parse::<u64>(),
            pid.parse::<u32>(),
        ) else {
            continue;
        };
        holders.insert((libc::makedev(major, minor), inode), pid);
    }
    holders
}

/// Whether a process is an app server bundled in a desktop app's resources, as Codex Desktop
/// runs one (`…/resources/codex` on Linux, `…/Contents/Resources/codex` on macOS).
pub fn bundled_app_server(pid: u32) -> bool {
    std::fs::read_link(format!("/proc/{pid}/exe")).is_ok_and(|exe| {
        exe.file_name().is_some_and(|name| name == "codex")
            && exe
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|dir| dir.eq_ignore_ascii_case("resources"))
    })
}

/// The kernel's boot ID, which tells a daemon PID file from before a reboot apart.
pub fn boot_id() -> Option<String> {
    std::fs::read_to_string("/proc/sys/kernel/random/boot_id")
        .ok()
        .map(|id| id.trim().to_owned())
}

/// The `codex` executable: `$PATH`, then the installer's usual places. A desktop session's
/// `PATH` often lacks `~/.local/bin`.
pub fn codex_executable() -> Option<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let mut candidates: Vec<PathBuf> = std::env::var_os("PATH")
        .map(|path| {
            std::env::split_paths(&path)
                .map(|dir| dir.join("codex"))
                .collect()
        })
        .unwrap_or_default();
    candidates.extend(home.map(|home| home.join(".local/bin/codex")));
    if cfg!(target_os = "macos") {
        candidates.extend(["/opt/homebrew/bin/codex", "/usr/local/bin/codex"].map(PathBuf::from));
    }
    candidates.into_iter().find(|path| is_executable(path))
}

fn is_executable(path: &Path) -> bool {
    use std::os::unix::fs::PermissionsExt;
    std::fs::metadata(path).is_ok_and(|m| m.is_file() && m.permissions().mode() & 0o111 != 0)
}

/// Claude Desktop's user data for this account: `$XDG_CONFIG_HOME/Claude` (default
/// `~/.config/Claude`) on Linux, `~/Library/Application Support/Claude` on macOS.
pub fn claude_desktop_data() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")
        .filter(|home| !home.is_empty())
        .map(PathBuf::from);
    if cfg!(target_os = "macos") {
        return home.map(|home| home.join("Library/Application Support/Claude"));
    }
    std::env::var_os("XDG_CONFIG_HOME")
        .filter(|dir| !dir.is_empty())
        .map(PathBuf::from)
        .or_else(|| home.map(|home| home.join(".config")))
        .map(|config| config.join("Claude"))
}

/// The application registered for a URL scheme, by its desktop entry. Linux asks `xdg-mime`;
/// macOS needs `LSCopyDefaultApplicationURLForURL`, which has not been verified.
pub async fn scheme_handler(scheme: &str) -> Option<String> {
    if !cfg!(target_os = "linux") {
        return None;
    }
    let mut command = Command::new("xdg-mime");
    command
        .args(["query", "default", &format!("x-scheme-handler/{scheme}")])
        .stdin(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(3), command.output())
        .await
        .ok()?
        .ok()?;
    let handler = String::from_utf8(output.stdout).ok()?.trim().to_owned();
    (output.status.success() && handler.ends_with(".desktop")).then_some(handler)
}

/// How a link is handed to the desktop: the first opener that exists is used.
#[derive(Clone, Debug)]
pub struct Opener {
    /// Each candidate is a program and its leading arguments; the link follows them.
    pub candidates: Vec<Vec<OsString>>,
}

/// What became of one attempt to open a link.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Opened {
    /// The opener reported success: the link reached its registered application.
    Handed,
    /// No opener could start; nothing left this process.
    NotStarted,
    /// The opener started but did not report success; the application may have the link.
    Uncertain,
}

impl Opener {
    /// `xdg-open`, else `gio open`, on Linux; `open` on macOS.
    pub fn system() -> Self {
        let candidates = if cfg!(target_os = "macos") {
            vec![vec!["open".into()]]
        } else {
            vec![vec!["xdg-open".into()], vec!["gio".into(), "open".into()]]
        };
        Self { candidates }
    }

    /// Open the link once. An opener that started is never followed by another.
    pub async fn open(&self, link: &str) -> Opened {
        for candidate in &self.candidates {
            let Some((program, arguments)) = candidate.split_first() else {
                continue;
            };
            let mut command = Command::new(program);
            command
                .args(arguments)
                .arg(link)
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null());
            let Ok(mut child) = command.spawn() else {
                continue;
            };
            return match tokio::time::timeout(Duration::from_secs(10), child.wait()).await {
                Ok(Ok(status)) if status.success() => Opened::Handed,
                _ => Opened::Uncertain,
            };
        }
        Opened::NotStarted
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_held_exclusive_flock_locks_have_holders() {
        let locks = "1: POSIX  ADVISORY  WRITE 100 00:1e:11 0 EOF\n\
                     2: FLOCK  ADVISORY  WRITE 954587 00:1e:39635243 0 EOF\n\
                     3: FLOCK  ADVISORY  READ 200 00:1e:22 0 EOF\n\
                     3: -> FLOCK  ADVISORY  WRITE 300 00:1e:39635243 0 EOF\n\
                     4: FLOCK  ADVISORY  WRITE 400 fd:01:77 0 EOF\n\
                     garbage line\n";
        let holders = parse_flock_holders(locks);
        assert_eq!(holders.len(), 2);
        assert_eq!(holders[&(libc::makedev(0, 0x1e), 39_635_243)], 954_587);
        assert_eq!(holders[&(libc::makedev(0xfd, 1), 77)], 400);
    }

    #[test]
    fn a_held_lock_in_this_process_is_found_through_the_kernel() {
        use std::os::unix::{fs::MetadataExt, io::AsRawFd};
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("thread.lock");
        let file = std::fs::File::create(&path).unwrap();
        let key = {
            let metadata = file.metadata().unwrap();
            (metadata.dev(), metadata.ino())
        };
        assert!(!flock_holders().contains_key(&key), "not held yet");
        // SAFETY: a valid open descriptor and a lock operation.
        assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
        assert_eq!(flock_holders().get(&key), Some(&std::process::id()));
        drop(file);
        assert!(
            !flock_holders().contains_key(&key),
            "released with the file"
        );
    }

    #[test]
    fn this_process_has_a_start_time_parent_and_namespace() {
        let own = std::process::id();
        assert!(process_start(own).is_some_and(|start| start.parse::<u64>().is_ok()));
        assert!(parent(own).is_some_and(|parent| parent > 0 && parent != own));
        assert!(own_namespace().is_some_and(|ns| ns.starts_with("pid:[")));
        assert!(!bundled_app_server(own));
    }

    #[tokio::test]
    async fn an_opener_hands_the_link_once_and_reports_what_it_knows() {
        let temp = tempfile::tempdir().unwrap();
        let log = temp.path().join("opened");
        let script = temp.path().join("opener");
        std::fs::write(
            &script,
            format!(
                "#!/bin/sh\nprintf '%s\\n' \"$@\" >> {}\nexit \"$EXIT\"\n",
                log.display()
            ),
        )
        .unwrap();
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&script, std::fs::Permissions::from_mode(0o700)).unwrap();
        let missing = temp.path().join("missing");
        let opener = |exit: &str| Opener {
            candidates: vec![
                vec![missing.clone().into()],
                vec![
                    "env".into(),
                    format!("EXIT={exit}").into(),
                    script.clone().into(),
                ],
            ],
        };
        assert_eq!(
            opener("0").open("claude://claude.ai/new?q=a%20b").await,
            Opened::Handed
        );
        assert_eq!(
            opener("4").open("claude://claude.ai/new?q=c").await,
            Opened::Uncertain
        );
        assert_eq!(
            std::fs::read_to_string(&log).unwrap(),
            "claude://claude.ai/new?q=a%20b\nclaude://claude.ai/new?q=c\n"
        );
        let nothing = Opener {
            candidates: vec![vec![missing.into()]],
        };
        assert_eq!(nothing.open("claude://x").await, Opened::NotStarted);
    }
}
