//! What differs between operating systems in reaching agent sessions: process identity, where
//! Claude Code's inboxes live, who holds a Codex thread's lock, which executable runs Codex and
//! how a link is opened.
//!
//! Linux and macOS are implemented; each macOS answer was checked against what Claude Code and
//! Codex record on a Mac. Elsewhere every function answers "unavailable", which callers treat as
//! no route, so an unknown platform shows its sessions as draft only rather than guessing.
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

/// A process's start second as Claude Code records it on macOS: `ps -o lstart` in UTC, such as
/// `Mon Oct 26 11:10:03 2026`, with single spaces (see [`recorded_start`]).
#[cfg(target_os = "macos")]
pub fn process_start(pid: u32) -> Option<String> {
    let seconds = libc::time_t::try_from(darwin::bsd_info(pid)?.pbi_start_tvsec).ok()?;
    darwin::lstart(seconds)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn process_start(_pid: u32) -> Option<String> {
    None
}

/// A recorded `procStart` in the form [`process_start`] returns. `ps` pads a single-digit day
/// of the month on macOS (`Tue Oct  6`), so the words are kept and the spacing is not.
pub fn recorded_start(recorded: &str) -> String {
    if cfg!(target_os = "macos") {
        recorded.split_whitespace().collect::<Vec<_>>().join(" ")
    } else {
        recorded.to_owned()
    }
}

/// A process's parent.
#[cfg(target_os = "linux")]
pub fn parent(pid: u32) -> Option<u32> {
    stat_fields(pid)?.get(1)?.parse().ok()
}

#[cfg(target_os = "macos")]
pub fn parent(pid: u32) -> Option<u32> {
    Some(darwin::bsd_info(pid)?.pbi_ppid)
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn parent(_pid: u32) -> Option<u32> {
    None
}

/// Whether the identity a Codex app server wrote in its PID file is this live process: its
/// start ticks on Linux, its start second and microsecond on macOS. The boot is compared
/// separately through [`boot_id`].
#[cfg(target_os = "linux")]
pub fn codex_identity_matches(identity: &serde_json::Value, pid: u32) -> bool {
    let recorded = identity
        .get("startTicks")
        .and_then(serde_json::Value::as_u64);
    recorded.is_some_and(|ticks| process_start(pid) == Some(ticks.to_string()))
}

#[cfg(target_os = "macos")]
pub fn codex_identity_matches(identity: &serde_json::Value, pid: u32) -> bool {
    let field = |key: &str| identity.get(key).and_then(serde_json::Value::as_u64);
    darwin::bsd_info(pid).is_some_and(|info| {
        field("startSeconds") == Some(info.pbi_start_tvsec)
            && field("startMicroseconds") == Some(info.pbi_start_tvusec)
    })
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn codex_identity_matches(_identity: &serde_json::Value, _pid: u32) -> bool {
    false
}

/// The fields of `/proc/<pid>/stat` after the parenthesised command name, from `state`.
#[cfg(target_os = "linux")]
fn stat_fields(pid: u32) -> Option<Vec<String>> {
    let stat = std::fs::read_to_string(format!("/proc/{pid}/stat")).ok()?;
    let rest = stat.get(stat.rfind(')')? + 1..)?;
    Some(rest.split_whitespace().map(str::to_owned).collect())
}

/// This process's PID domain, which a Claude Code registry entry's `pidDomain` must share: the
/// PID namespace on Linux, such as `pid:[4026531836]`; on macOS, which has none, `darwin`.
pub fn own_namespace() -> Option<String> {
    if cfg!(target_os = "macos") {
        return Some("darwin".into());
    }
    std::fs::read_link("/proc/self/ns/pid")
        .ok()
        .map(|link| link.to_string_lossy().into_owned())
}

/// Whether a registry entry's `pidDomain` is [`own_namespace`]. Linux entries end with the
/// namespace of the process that registered (`linux:<machine>:pid:[…]`); macOS entries are the
/// bare domain.
pub fn same_domain(recorded: &str, ours: &str) -> bool {
    if cfg!(target_os = "macos") {
        return recorded == ours;
    }
    recorded
        .find("pid:[")
        .is_some_and(|at| &recorded[at..] == ours)
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

/// Processes holding exclusive `flock` locks, by the locked file's device and inode as the
/// kernel names them. On btrfs every subvolume shares one device there and inode numbers repeat
/// across subvolumes, so one key can name several files, each with its own holder.
pub type FlockHolders = HashMap<(u64, u64), Vec<u32>>;

/// Holders of exclusive `flock` locks from `/proc/locks` on Linux. A lock file left behind by a
/// process that exited has no holder.
#[cfg(target_os = "linux")]
pub fn flock_holders() -> FlockHolders {
    std::fs::read_to_string("/proc/locks")
        .map(|locks| parse_flock_holders(&locks))
        .unwrap_or_default()
}

/// Holders of `flock` locks among this account's processes on macOS, which has no lock table
/// to read. A descriptor that holds one carries the kernel's `FHASLOCK` open flag, which
/// `proc_pidfdinfo` reports, so each process's open files are read. Shared and exclusive locks
/// look alike there; Codex takes its thread locks only exclusively.
#[cfg(target_os = "macos")]
pub fn flock_holders() -> FlockHolders {
    darwin::flock_holders()
}

#[cfg(not(any(target_os = "linux", target_os = "macos")))]
pub fn flock_holders() -> FlockHolders {
    HashMap::new()
}

/// The process holding an exclusive `flock` on the file at `path`, given its metadata and the
/// holders from [`flock_holders`]. The kernel names a lock's file by its mount's device, which
/// on btrfs differs from the per-subvolume device `stat` reports, so both are tried.
pub fn flock_holder(
    holders: &FlockHolders,
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Option<u32> {
    let mount = path.parent().and_then(mount_device);
    flock_holder_on(holders, mount, path, metadata)
}

/// As [`flock_holder`], with the mount's device already known, for many files in one
/// directory. A match through the mount's device names only an inode on that filesystem, which
/// files in other subvolumes can share, so the holder is the one that has this file open.
pub fn flock_holder_on(
    holders: &FlockHolders,
    mount: Option<u64>,
    path: &Path,
    metadata: &std::fs::Metadata,
) -> Option<u32> {
    use std::os::unix::fs::MetadataExt;
    let inode = metadata.ino();
    match holders.get(&(metadata.dev(), inode)).map(Vec::as_slice) {
        Some([holder]) => return Some(*holder),
        Some(several) if !several.is_empty() => {
            return several.iter().copied().find(|pid| holds_open(*pid, path));
        }
        _ => {}
    }
    holders
        .get(&(mount?, inode))?
        .iter()
        .copied()
        .find(|pid| holds_open(*pid, path))
}

/// Whether a process has the file at `path` open, from its descriptors in `proc_pidfdinfo`.
#[cfg(target_os = "macos")]
fn holds_open(pid: u32, path: &Path) -> bool {
    use std::os::unix::fs::MetadataExt;
    std::fs::metadata(path).is_ok_and(|file| darwin::has_open(pid, file.dev(), file.ino()))
}

/// Whether a process has the file at `path` open, from its descriptors in `/proc`.
#[cfg(not(target_os = "macos"))]
fn holds_open(pid: u32, path: &Path) -> bool {
    let Ok(path) = std::fs::canonicalize(path) else {
        return false;
    };
    std::fs::read_dir(format!("/proc/{pid}/fd")).is_ok_and(|descriptors| {
        descriptors
            .take(65_536)
            .flatten()
            .any(|fd| std::fs::read_link(fd.path()).is_ok_and(|target| target == path))
    })
}

/// The device of the mount holding `directory`, from `/proc/self/mountinfo`; Linux only.
pub fn mount_device(directory: &Path) -> Option<u64> {
    let directory = std::fs::canonicalize(directory).ok()?;
    let mounts = std::fs::read_to_string("/proc/self/mountinfo").ok()?;
    mount_device_in(&mounts, &directory)
}

/// The device of the deepest mount point containing `path` in a `mountinfo` table.
pub(crate) fn mount_device_in(mounts: &str, path: &Path) -> Option<u64> {
    mounts
        .lines()
        .filter_map(|line| {
            let mut fields = line.split(' ');
            let device = fields.nth(2)?;
            let point = unescape_mount(fields.nth(1)?);
            path.starts_with(&point).then_some((point, device))
        })
        .max_by_key(|(point, _)| point.as_os_str().len())
        .and_then(|(_, device)| {
            let (major, minor) = device.split_once(':')?;
            Some(linux_device(major.parse().ok()?, minor.parse().ok()?))
        })
}

/// The device number for a `major:minor` pair as `/proc` prints it, in Linux's encoding, so
/// the parsers below behave the same wherever they are built and tested.
fn linux_device(major: u32, minor: u32) -> u64 {
    #[cfg(target_os = "linux")]
    return libc::makedev(major, minor);
    #[cfg(not(target_os = "linux"))]
    {
        let (major, minor) = (u64::from(major), u64::from(minor));
        ((major & 0x0000_0fff) << 8)
            | ((major & 0xffff_f000) << 32)
            | (minor & 0x0000_00ff)
            | ((minor & 0xffff_ff00) << 12)
    }
}

/// `mountinfo` writes spaces and other separators in paths as octal escapes such as `\040`.
fn unescape_mount(field: &str) -> PathBuf {
    use std::os::unix::ffi::OsStringExt;
    let bytes = field.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut at = 0;
    while at < bytes.len() {
        let octal = bytes.get(at + 1..at + 4).filter(|digits| {
            bytes[at] == b'\\' && digits.iter().all(|d| (b'0'..=b'7').contains(d))
        });
        match octal {
            Some(digits) => {
                out.push(digits.iter().fold(0u8, |n, d| n * 8 + (d - b'0')));
                at += 4;
            }
            None => {
                out.push(bytes[at]);
                at += 1;
            }
        }
    }
    PathBuf::from(OsString::from_vec(out))
}

/// Parse `/proc/locks` lines such as `15: FLOCK  ADVISORY  WRITE 954587 00:1e:39635243 0 EOF`
/// into (device, inode) → PID, for exclusive `flock` locks only. Blocked waiters (`->`) are not
/// holders.
#[cfg_attr(
    not(target_os = "linux"),
    allow(
        dead_code,
        reason = "only Linux reads /proc/locks; the tests cover the parser everywhere"
    )
)]
pub(crate) fn parse_flock_holders(locks: &str) -> FlockHolders {
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
        holders
            .entry((linux_device(major, minor), inode))
            .or_insert_with(Vec::new)
            .push(pid);
    }
    holders
}

/// Whether a process is an app server bundled in a desktop app's resources, as Codex Desktop
/// runs one (`…/resources/codex` on Linux, `…/Contents/Resources/codex` on macOS).
pub fn bundled_app_server(pid: u32) -> bool {
    executable(pid).is_some_and(|exe| {
        exe.file_name().is_some_and(|name| name == "codex")
            && exe
                .parent()
                .and_then(Path::file_name)
                .is_some_and(|dir| dir.eq_ignore_ascii_case("resources"))
    })
}

/// A process's executable: `/proc/<pid>/exe` on Linux, `proc_pidpath` on macOS.
fn executable(pid: u32) -> Option<PathBuf> {
    #[cfg(target_os = "macos")]
    return darwin::executable(pid);
    #[cfg(not(target_os = "macos"))]
    std::fs::read_link(format!("/proc/{pid}/exe")).ok()
}

/// The kernel's boot ID, which tells a daemon PID file from before a reboot apart:
/// `/proc/sys/kernel/random/boot_id` on Linux, `kern.bootsessionuuid` on macOS, as Codex
/// records them.
pub fn boot_id() -> Option<String> {
    #[cfg(target_os = "macos")]
    return darwin::boot_session();
    #[cfg(not(target_os = "macos"))]
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

/// The application registered for a URL scheme, by its bundle, from Launch Services.
#[cfg(target_os = "macos")]
pub async fn scheme_handler(scheme: &str) -> Option<String> {
    darwin::scheme_handler(scheme).filter(|app| app.ends_with(".app"))
}

/// The application registered for a URL scheme, by its desktop entry, from `xdg-mime`.
#[cfg(not(target_os = "macos"))]
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

/// How long an opener may take to report failure before it counts as having launched.
const OPENER_GRACE: Duration = Duration::from_millis(1_500);

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

    /// Open the link once. An opener that started is never followed by another. Some openers
    /// stay running for as long as the application they started (`xdg-open` on Hyprland), so one
    /// still running after a moment has launched its handler; it is reaped in the background.
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
            return match tokio::time::timeout(OPENER_GRACE, child.wait()).await {
                Ok(Ok(status)) if status.success() => Opened::Handed,
                Ok(_) => Opened::Uncertain,
                Err(_) => {
                    tokio::spawn(async move {
                        let _ = child.wait().await;
                    });
                    Opened::Handed
                }
            };
        }
        Opened::NotStarted
    }
}

/// The macOS kernel's process table, through `libproc`, and Launch Services.
#[cfg(target_os = "macos")]
mod darwin {
    use super::FlockHolders;
    use std::{
        ffi::{CStr, c_int, c_void},
        mem::{MaybeUninit, size_of},
        path::PathBuf,
    };

    /// Bounds against a crowded process table; a person runs far fewer.
    const MAX_PROCESSES: usize = 16_384;
    const MAX_DESCRIPTORS: usize = 65_536;
    /// The open-file flag of a descriptor that holds an advisory `flock` lock (`FHASLOCK` in
    /// XNU's `sys/fcntl.h`), which `proc_fileinfo.fi_openflags` carries.
    const FHASLOCK: u32 = 0x4000;
    /// `proc_pidfdinfo` flavour for a vnode descriptor's file and vnode facts.
    const PROC_PIDFDVNODEINFO: c_int = 1;

    /// `struct proc_fileinfo` from `sys/proc_info.h`.
    #[repr(C)]
    struct FileInfo {
        openflags: u32,
        status: u32,
        offset: libc::off_t,
        kind: i32,
        guardflags: u32,
    }

    /// `struct vnode_fdinfo` from `sys/proc_info.h`.
    #[repr(C)]
    struct VnodeDescriptor {
        file: FileInfo,
        vnode: libc::vnode_info,
    }

    pub(super) fn bsd_info(pid: u32) -> Option<libc::proc_bsdinfo> {
        let pid = c_int::try_from(pid).ok()?;
        let mut info = MaybeUninit::<libc::proc_bsdinfo>::zeroed();
        let size = c_int::try_from(size_of::<libc::proc_bsdinfo>()).ok()?;
        // SAFETY: the buffer is exactly one proc_bsdinfo and outlives the call.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDTBSDINFO,
                0,
                info.as_mut_ptr().cast(),
                size,
            )
        };
        // SAFETY: the kernel filled the whole value; zeroed is valid plain data either way.
        (written == size).then(|| unsafe { info.assume_init() })
    }

    /// `%a %b %e %H:%M:%S %Y` in UTC with single spaces, independent of the locale.
    pub(super) fn lstart(seconds: libc::time_t) -> Option<String> {
        const DAYS: [&str; 7] = ["Sun", "Mon", "Tue", "Wed", "Thu", "Fri", "Sat"];
        const MONTHS: [&str; 12] = [
            "Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec",
        ];
        let mut tm = MaybeUninit::<libc::tm>::zeroed();
        // SAFETY: gmtime_r writes only into the tm it is given.
        if unsafe { libc::gmtime_r(&seconds, tm.as_mut_ptr()) }.is_null() {
            return None;
        }
        // SAFETY: gmtime_r succeeded and filled it.
        let tm = unsafe { tm.assume_init() };
        Some(format!(
            "{} {} {} {:02}:{:02}:{:02} {}",
            DAYS.get(usize::try_from(tm.tm_wday).ok()?)?,
            MONTHS.get(usize::try_from(tm.tm_mon).ok()?)?,
            tm.tm_mday,
            tm.tm_hour,
            tm.tm_min,
            tm.tm_sec,
            tm.tm_year + 1900
        ))
    }

    pub(super) fn executable(pid: u32) -> Option<PathBuf> {
        let pid = c_int::try_from(pid).ok()?;
        let mut path = vec![0u8; libc::PROC_PIDPATHINFO_MAXSIZE as usize];
        let size = u32::try_from(path.len()).ok()?;
        // SAFETY: the buffer holds `size` bytes and outlives the call.
        let written = unsafe { libc::proc_pidpath(pid, path.as_mut_ptr().cast(), size) };
        path.truncate(usize::try_from(written).ok().filter(|n| *n > 0)?);
        Some(PathBuf::from(String::from_utf8(path).ok()?))
    }

    pub(super) fn boot_session() -> Option<String> {
        let mut value = [0u8; 64];
        let mut length = value.len();
        // SAFETY: the name is NUL-terminated and the buffer holds `length` bytes.
        let status = unsafe {
            libc::sysctlbyname(
                c"kern.bootsessionuuid".as_ptr(),
                value.as_mut_ptr().cast(),
                &mut length,
                std::ptr::null_mut(),
                0,
            )
        };
        if status != 0 {
            return None;
        }
        let id = CStr::from_bytes_until_nul(value.get(..length)?).ok()?;
        Some(id.to_str().ok()?.trim().to_owned()).filter(|id| !id.is_empty())
    }

    /// Every process ID the kernel lists.
    fn processes() -> Vec<u32> {
        // SAFETY: a null buffer asks only for the count.
        let count = unsafe { libc::proc_listallpids(std::ptr::null_mut(), 0) };
        let Some(count) = usize::try_from(count).ok().filter(|n| *n > 0) else {
            return vec![];
        };
        let mut pids: Vec<libc::pid_t> = vec![0; (count + 64).min(MAX_PROCESSES)];
        let Ok(bytes) = c_int::try_from(pids.len() * size_of::<libc::pid_t>()) else {
            return vec![];
        };
        // SAFETY: the buffer holds `bytes` bytes and outlives the call.
        let listed = unsafe { libc::proc_listallpids(pids.as_mut_ptr().cast(), bytes) };
        pids.truncate(usize::try_from(listed).unwrap_or(0));
        pids.into_iter()
            .filter_map(|pid| u32::try_from(pid).ok().filter(|pid| *pid > 0))
            .collect()
    }

    /// A process's open vnode descriptors.
    fn vnode_descriptors(pid: c_int) -> Vec<i32> {
        // SAFETY: a null buffer asks only for the size the list needs.
        let needed =
            unsafe { libc::proc_pidinfo(pid, libc::PROC_PIDLISTFDS, 0, std::ptr::null_mut(), 0) };
        let Some(needed) = usize::try_from(needed).ok().filter(|n| *n > 0) else {
            return vec![];
        };
        let slots = (needed / size_of::<libc::proc_fdinfo>() + 16).min(MAX_DESCRIPTORS);
        let mut descriptors = vec![
            libc::proc_fdinfo {
                proc_fd: 0,
                proc_fdtype: 0
            };
            slots
        ];
        let Ok(bytes) = c_int::try_from(slots * size_of::<libc::proc_fdinfo>()) else {
            return vec![];
        };
        // SAFETY: the buffer holds `bytes` bytes and outlives the call.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDLISTFDS,
                0,
                descriptors.as_mut_ptr().cast(),
                bytes,
            )
        };
        descriptors
            .truncate(usize::try_from(written).unwrap_or(0) / size_of::<libc::proc_fdinfo>());
        descriptors
            .into_iter()
            .filter(|fd| fd.proc_fdtype == libc::PROX_FDTYPE_VNODE as u32)
            .map(|fd| fd.proc_fd)
            .collect()
    }

    fn vnode(pid: c_int, fd: i32) -> Option<VnodeDescriptor> {
        let mut info = MaybeUninit::<VnodeDescriptor>::zeroed();
        let size = c_int::try_from(size_of::<VnodeDescriptor>()).ok()?;
        // SAFETY: the buffer is exactly one vnode_fdinfo and outlives the call.
        let written = unsafe {
            libc::proc_pidfdinfo(
                pid,
                fd,
                PROC_PIDFDVNODEINFO,
                info.as_mut_ptr().cast::<c_void>(),
                size,
            )
        };
        // SAFETY: the kernel filled the whole value; zeroed is valid plain data either way.
        (written == size).then(|| unsafe { info.assume_init() })
    }

    /// Whether a process has a descriptor open on the file with this device and inode.
    pub(super) fn has_open(pid: u32, device: u64, inode: u64) -> bool {
        let Ok(raw) = c_int::try_from(pid) else {
            return false;
        };
        vnode_descriptors(raw).into_iter().any(|fd| {
            vnode(raw, fd).is_some_and(|open| {
                let stat = open.vnode.vi_stat;
                u64::from(stat.vst_dev) == device && stat.vst_ino == inode
            })
        })
    }

    /// The files this account's processes hold `flock` locks on, by device and inode as `stat`
    /// reports them. Another account's processes cannot be read and hold none of its files.
    pub(super) fn flock_holders() -> FlockHolders {
        let uid = super::effective_uid();
        let mut holders = FlockHolders::new();
        for pid in processes() {
            let (Some(info), Ok(raw)) = (bsd_info(pid), c_int::try_from(pid)) else {
                continue;
            };
            if info.pbi_uid != uid {
                continue;
            }
            for fd in vnode_descriptors(raw) {
                let Some(open) = vnode(raw, fd).filter(|open| open.file.openflags & FHASLOCK != 0)
                else {
                    continue;
                };
                let stat = open.vnode.vi_stat;
                let held = holders
                    .entry((u64::from(stat.vst_dev), stat.vst_ino))
                    .or_default();
                if !held.contains(&pid) {
                    held.push(pid);
                }
            }
        }
        holders
    }

    type CFTypeRef = *const c_void;
    type CFIndex = isize;
    const UTF8: u32 = 0x0800_0100;
    const ALL_ROLES: u32 = 0xFFFF_FFFF;

    #[link(name = "CoreFoundation", kind = "framework")]
    unsafe extern "C" {
        fn CFURLCreateWithBytes(
            allocator: CFTypeRef,
            bytes: *const u8,
            length: CFIndex,
            encoding: u32,
            base: CFTypeRef,
        ) -> CFTypeRef;
        fn CFURLGetFileSystemRepresentation(
            url: CFTypeRef,
            resolve: u8,
            buffer: *mut u8,
            length: CFIndex,
        ) -> u8;
        fn CFRelease(value: CFTypeRef);
    }

    #[link(name = "CoreServices", kind = "framework")]
    unsafe extern "C" {
        fn LSCopyDefaultApplicationURLForURL(
            url: CFTypeRef,
            roles: u32,
            error: *mut CFTypeRef,
        ) -> CFTypeRef;
    }

    /// The bundle Launch Services opens `<scheme>:` links with, such as `/Applications/Claude.app`.
    pub(super) fn scheme_handler(scheme: &str) -> Option<String> {
        if scheme.is_empty()
            || !scheme
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'-' | b'.'))
        {
            return None;
        }
        let link = format!("{scheme}://");
        // SAFETY: the bytes outlive the call; a null allocator and base are the defaults.
        let url = unsafe {
            CFURLCreateWithBytes(
                std::ptr::null(),
                link.as_ptr(),
                link.len() as CFIndex,
                UTF8,
                std::ptr::null(),
            )
        };
        if url.is_null() {
            return None;
        }
        // SAFETY: a valid URL; a null error pointer is allowed.
        let app =
            unsafe { LSCopyDefaultApplicationURLForURL(url, ALL_ROLES, std::ptr::null_mut()) };
        // SAFETY: created above and released once.
        unsafe { CFRelease(url) };
        if app.is_null() {
            return None;
        }
        let mut path = vec![0u8; libc::PATH_MAX as usize];
        // SAFETY: the buffer holds its length in bytes and outlives the call.
        let converted = unsafe {
            CFURLGetFileSystemRepresentation(app, 1, path.as_mut_ptr(), path.len() as CFIndex)
        };
        // SAFETY: copied by the call above and released once.
        unsafe { CFRelease(app) };
        if converted == 0 {
            return None;
        }
        let path = CStr::from_bytes_until_nul(&path).ok()?.to_str().ok()?;
        Some(path.trim_end_matches('/').to_owned())
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
        assert_eq!(holders[&(linux_device(0, 0x1e), 39_635_243)], [954_587]);
        assert_eq!(holders[&(linux_device(0xfd, 1), 77)], [400]);
        // Two btrfs subvolumes, one device in the kernel's table, the same inode: both kept.
        let shared = parse_flock_holders(
            "1: FLOCK  ADVISORY  WRITE 10 00:1e:77 0 EOF\n2: FLOCK  ADVISORY  WRITE 20 00:1e:77 0 EOF\n",
        );
        assert_eq!(shared[&(linux_device(0, 0x1e), 77)], [10, 20]);
    }

    #[test]
    fn of_several_holders_of_one_inode_the_one_with_this_file_open_holds_it() {
        use std::os::unix::fs::MetadataExt;
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("thread.lock");
        let _open = std::fs::File::create(&path).unwrap();
        let metadata = std::fs::symlink_metadata(&path).unwrap();
        // A device the file is not on, as the kernel names a btrfs subvolume's files.
        let mount = linux_device(0, 0x7ff);
        assert_ne!(metadata.dev(), mount);
        let own = std::process::id();
        // Process 1 holds another file with the same inode elsewhere; it never has this one open.
        let holders: FlockHolders = [((mount, metadata.ino()), vec![1, own])].into();
        assert_eq!(
            flock_holder_on(&holders, Some(mount), &path, &metadata),
            Some(own)
        );
        let elsewhere: FlockHolders = [((mount, metadata.ino()), vec![1])].into();
        assert_eq!(
            flock_holder_on(&elsewhere, Some(mount), &path, &metadata),
            None
        );
    }

    #[test]
    fn a_file_is_on_its_deepest_mount() {
        let mounts = "26 33 0:24 / /proc rw - proc proc rw\n\
                      33 2 0:30 /@ / rw shared:1 - btrfs /dev/mapper/root rw\n\
                      64 33 0:30 /@home /home rw shared:197 - btrfs /dev/mapper/root rw\n\
                      90 64 0:77 / /home/u/My\\040Drive rw - tmpfs tmpfs rw\n";
        assert_eq!(
            mount_device_in(mounts, Path::new("/home/u/.codex/thread-writer-locks")),
            Some(linux_device(0, 30))
        );
        assert_eq!(
            mount_device_in(mounts, Path::new("/home/u/My Drive/x")),
            Some(linux_device(0, 77))
        );
        assert_eq!(
            mount_device_in(mounts, Path::new("/proc/locks")),
            Some(linux_device(0, 24))
        );
    }

    #[test]
    fn a_held_lock_in_this_process_is_found_through_the_kernel() {
        use std::os::unix::io::AsRawFd;
        // A temporary directory, and this package's own, which may be a btrfs subvolume whose
        // `stat` device differs from the one in the kernel's lock table.
        let package = tempfile::tempdir_in(std::env::current_dir().unwrap()).unwrap();
        let temporary = tempfile::tempdir().unwrap();
        for directory in [temporary.path(), package.path()] {
            let path = directory.join("thread.lock");
            let file = std::fs::File::create(&path).unwrap();
            let holder = || {
                let metadata = std::fs::symlink_metadata(&path).unwrap();
                flock_holder(&flock_holders(), &path, &metadata)
            };
            assert_eq!(holder(), None, "not held yet");
            // SAFETY: a valid open descriptor and a lock operation.
            assert_eq!(unsafe { libc::flock(file.as_raw_fd(), libc::LOCK_EX) }, 0);
            assert_eq!(
                holder(),
                Some(std::process::id()),
                "{}",
                directory.display()
            );
            drop(file);
            // A child another test thread forks inherits the descriptor until it execs, so the
            // lock can outlive the file here by a moment.
            let released = std::time::Instant::now() + Duration::from_secs(2);
            while holder().is_some() && std::time::Instant::now() < released {
                std::thread::sleep(Duration::from_millis(10));
            }
            assert_eq!(holder(), None, "released with the file");
        }
    }

    #[test]
    fn this_process_has_a_start_time_parent_and_namespace() {
        let own = std::process::id();
        let start = process_start(own).unwrap();
        assert!(parent(own).is_some_and(|parent| parent > 0 && parent != own));
        assert!(!bundled_app_server(own));
        assert!(
            boot_id().is_some_and(|id| id.len() == 36),
            "{:?}",
            boot_id()
        );
        if cfg!(target_os = "macos") {
            assert_eq!(start.split_whitespace().count(), 5, "{start}");
            assert_eq!(own_namespace().as_deref(), Some("darwin"));
        } else {
            assert!(start.parse::<u64>().is_ok(), "{start}");
            assert!(own_namespace().is_some_and(|ns| ns.starts_with("pid:[")));
        }
    }

    #[test]
    fn a_registry_domain_matches_only_this_platforms_own() {
        let ours = own_namespace().unwrap();
        let recorded = if cfg!(target_os = "macos") {
            ours.clone()
        } else {
            format!("linux:0123456789abcdef0123456789abcdef:{ours}")
        };
        assert!(same_domain(&recorded, &ours));
        assert!(!same_domain(
            "linux:0123456789abcdef0123456789abcdef:pid:[1]",
            &ours
        ));
        assert!(!same_domain("", &ours));
    }

    /// Claude Code records `procStart` on macOS as `ps -o lstart` prints it in UTC.
    #[cfg(target_os = "macos")]
    #[test]
    fn macos_start_times_read_as_claude_code_records_them() {
        assert_eq!(
            darwin::lstart(1_793_013_003).as_deref(),
            Some("Mon Oct 26 11:10:03 2026")
        );
        // `ps` pads a single-digit day; the recorded form is compared by its words.
        assert_eq!(
            darwin::lstart(1_791_282_000).as_deref(),
            Some("Tue Oct 6 10:20:00 2026")
        );
        assert_eq!(
            recorded_start("Tue Oct  6 10:20:00 2026"),
            "Tue Oct 6 10:20:00 2026"
        );
        let own = std::process::id();
        let printed = std::process::Command::new("ps")
            .args(["-o", "lstart=", "-p", &own.to_string()])
            .env("TZ", "UTC")
            .env("LC_ALL", "C")
            .output()
            .unwrap();
        let printed = String::from_utf8(printed.stdout).unwrap();
        assert_eq!(recorded_start(&printed), process_start(own).unwrap());
    }

    /// The peer check every inbox and app-server connection makes, against a socket this process
    /// serves itself.
    #[tokio::test]
    async fn a_connected_unix_peer_names_its_user_and_process() {
        let temp = tempfile::tempdir().unwrap();
        let path = temp.path().join("peer.sock");
        let listener = tokio::net::UnixListener::bind(&path).unwrap();
        let (client, served) =
            tokio::join!(tokio::net::UnixStream::connect(&path), listener.accept());
        let (client, (server, _)) = (client.unwrap(), served.unwrap());
        let own = (effective_uid(), std::process::id());
        assert_eq!(peer_process(&client), Some(own));
        assert_eq!(peer_process(&server), Some(own));
    }

    #[cfg(target_os = "macos")]
    #[tokio::test]
    async fn launch_services_names_the_app_for_a_scheme() {
        let browser = scheme_handler("https").await;
        assert!(
            browser
                .as_deref()
                .is_some_and(|app| app.starts_with('/') && app.ends_with(".app")),
            "{browser:?}"
        );
        assert_eq!(scheme_handler("mluva-no-such-scheme-7f3a").await, None);
        assert_eq!(scheme_handler("bad scheme").await, None);
        let own = executable(std::process::id()).unwrap();
        assert_eq!(
            own,
            std::env::current_exe().unwrap().canonicalize().unwrap()
        );
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
        // An opener that stays with the application it launched counts as handed, at once.
        let staying = Opener {
            candidates: vec![vec![
                "sh".into(),
                "-c".into(),
                "sleep 30".into(),
                "opener".into(),
            ]],
        };
        let started = std::time::Instant::now();
        assert_eq!(staying.open("claude://x").await, Opened::Handed);
        assert!(started.elapsed() < Duration::from_secs(3));
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
