//! Which Codex app server Mluva talks to.
//!
//! Every app server on the same `CODEX_HOME` shares Codex's queue store, so any of them can
//! queue a message for a thread another process owns. Mluva uses the shared daemon's control
//! socket when the daemon runs and answers. Otherwise it starts its own `codex app-server`,
//! listening only on a socket in Mluva's private runtime directory, as a child process in its
//! own process group: no installed service and never a managed daemon. It is started when
//! needed, stopped by its own watchdog after ten idle minutes and when Mluva closes, and offered
//! to no other client, so it hosts nobody's threads. A launcher such as `npm`'s `codex`, which
//! runs the real server as its own child, is accepted: the server must descend from the process
//! Mluva started, and stopping the group stops both.
//!
//! An app server writes `config.toml` only when a thread is started with full access (it then
//! records the project as trusted) or when asked to change configuration. Mluva's client can
//! send none of those methods ([`super::rpc::ALLOWED_METHODS`]). Codex itself rewrites the file
//! for its own reasons, so a change is no signal; the one change attributable to the helper,
//! a trust entry for its own working directory, is reported if it ever appears.
use super::rpc::{AppServer, Peer};
use crate::{agent_platform, claude_sessions, process_lifetime};
use serde_json::Value;
use std::{
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        Arc, OnceLock,
        atomic::{AtomicBool, AtomicUsize, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{
    process::Command,
    sync::{Mutex, Notify},
};

/// A helper nobody used for this long is stopped.
const HELPER_IDLE: Duration = Duration::from_secs(10 * 60);
/// How often the helper's watchdog looks, at most.
const IDLE_CHECK: Duration = Duration::from_secs(60);
/// How long a starting helper may take to answer.
const HELPER_START: Duration = Duration::from_secs(15);
/// How long closing waits for deliveries in flight, such as a withdrawal, before stopping the
/// helper.
const SHUTDOWN_DRAIN: Duration = Duration::from_secs(5);
const MAX_PID_FILE: u64 = 16 * 1_024;
const MAX_CONFIG: u64 = 16 * 1_048_576;

/// Which server answered.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Owner {
    /// The shared daemon, which owns the threads of daemon-attached TUIs.
    Daemon,
    /// Mluva's own helper, which owns no threads.
    Helper,
}

/// Where Mluva finds and starts Codex app servers.
pub struct Servers {
    home: PathBuf,
    /// Mluva's private runtime directory for the helper's socket and working directory.
    runtime: Option<PathBuf>,
    codex: Option<PathBuf>,
    helper: Arc<Mutex<Option<Helper>>>,
    idle: Duration,
    /// Mluva is closing: no helper starts any more.
    closing: AtomicBool,
    /// Deliveries in flight, which closing lets finish.
    deliveries: AtomicUsize,
    drained: Notify,
    /// A test's own app server, served by the test process, standing in for the daemon.
    #[cfg(test)]
    fixed: Option<PathBuf>,
}

struct Helper {
    child: tokio::process::Child,
    /// The process Mluva started, which leads its own process group.
    pid: u32,
    rendezvous: PathBuf,
    used: Instant,
    /// Whether Codex already trusted the helper's working directory before it started.
    trusted_before: bool,
}

/// A delivery in flight; closing waits for it.
pub struct InFlight<'a>(&'a Servers);

impl Drop for InFlight<'_> {
    fn drop(&mut self) {
        self.0.deliveries.fetch_sub(1, Ordering::AcqRel);
        self.0.drained.notify_waiters();
    }
}

impl Servers {
    pub fn new(home: PathBuf, runtime: Option<PathBuf>, codex: Option<PathBuf>) -> Self {
        Self {
            home,
            runtime,
            codex,
            helper: Arc::new(Mutex::new(None)),
            idle: HELPER_IDLE,
            closing: AtomicBool::new(false),
            deliveries: AtomicUsize::new(0),
            drained: Notify::new(),
            #[cfg(test)]
            fixed: None,
        }
    }

    /// The same servers, with the helper stopped after `idle` unused.
    pub fn with_idle(mut self, idle: Duration) -> Self {
        self.idle = idle;
        self
    }

    /// Servers whose daemon is a test's own app server at `socket`.
    #[cfg(test)]
    pub(crate) fn fixed(home: PathBuf, socket: PathBuf) -> Self {
        Self {
            fixed: Some(socket),
            ..Self::new(home, None, None)
        }
    }

    /// The process's servers, from `CODEX_HOME` (default `~/.codex`), `XDG_RUNTIME_DIR` and the
    /// `codex` executable.
    pub fn system() -> Option<&'static Self> {
        static SERVERS: OnceLock<Option<Servers>> = OnceLock::new();
        SERVERS
            .get_or_init(|| {
                let home = super::codex_home()?;
                let runtime = std::env::var_os("XDG_RUNTIME_DIR")
                    .filter(|dir| !dir.is_empty())
                    .map(|dir| PathBuf::from(dir).join("mluva").join("codex"));
                Some(Self::new(home, runtime, agent_platform::codex_executable()))
            })
            .as_ref()
    }

    pub fn home(&self) -> &Path {
        &self.home
    }

    /// An initialized connection: the daemon when it answers, else the helper, started if
    /// needed. `None` when neither is available.
    pub async fn connect(&self) -> Option<(AppServer, Owner)> {
        self.connect_as(true).await
    }

    /// As [`Self::connect`]; the helper is used, and started, only when `helper` allows. The
    /// daemon is tried once.
    pub async fn connect_as(&self, helper: bool) -> Option<(AppServer, Owner)> {
        if let Some(server) = self.connect_daemon().await {
            return Some((server, Owner::Daemon));
        }
        if !helper {
            return None;
        }
        self.helper().await.map(|server| (server, Owner::Helper))
    }

    /// The daemon alone, when it runs and answers; it never starts anything.
    async fn connect_daemon(&self) -> Option<AppServer> {
        #[cfg(test)]
        if let Some(socket) = &self.fixed {
            return AppServer::connect(socket, Peer::Process(std::process::id()))
                .await
                .ok();
        }
        let (socket, pid) = self.daemon()?;
        AppServer::connect(&socket, Peer::Process(pid)).await.ok()
    }

    /// The daemon's process, when its PID file names a live process from this boot.
    pub fn daemon_pid(&self) -> Option<u32> {
        let directory = self.home.join("app-server-daemon");
        let file = ["daemon.pid", "app-server.pid"]
            .iter()
            .map(|name| directory.join(name))
            .find(|path| path.exists())?;
        let (file, metadata) = claude_sessions::open_private(&file)?;
        if metadata.len() > MAX_PID_FILE {
            return None;
        }
        let mut bytes = Vec::new();
        file.take(MAX_PID_FILE).read_to_end(&mut bytes).ok()?;
        let record: Value = serde_json::from_slice(&bytes).ok()?;
        let pid = u32::try_from(record.get("pid")?.as_u64()?).ok()?;
        let identity = record.get("processIdentity")?;
        // A PID file left from before a reboot, or a PID now used by another process, is no
        // daemon.
        (identity.get("bootId")?.as_str()? == agent_platform::boot_id()?
            && identity.get("startTicks")?.as_u64()?.to_string()
                == agent_platform::process_start(pid)?)
        .then_some(pid)
    }

    /// The daemon's socket and process, when it runs and its control socket is private to this
    /// account.
    fn daemon(&self) -> Option<(PathBuf, u32)> {
        let pid = self.daemon_pid()?;
        let socket =
            rendezvous_target(&self.home.join("app-server-control/app-server-control.sock"))?;
        Some((socket, pid))
    }

    /// Mluva's own helper, started if it is not running and Mluva is not closing.
    async fn helper(&self) -> Option<AppServer> {
        if self.closing.load(Ordering::Acquire) {
            return None;
        }
        let mut helper = self.helper.lock().await;
        if let Some(running) = helper.as_mut() {
            let alive = matches!(running.child.try_wait(), Ok(None));
            if alive && let Some(socket) = rendezvous_target(&running.rendezvous) {
                running.used = Instant::now();
                if let Ok(server) = AppServer::connect(&socket, Peer::Within(running.pid)).await {
                    return Some(server);
                }
            }
            if let Some(stale) = helper.take() {
                stop(stale, &self.home).await;
            }
        }
        if self.closing.load(Ordering::Acquire) {
            return None;
        }
        let (started, server) = self.start().await?;
        *helper = Some(started);
        drop(helper);
        tokio::spawn(watchdog(self.helper.clone(), self.idle, self.home.clone()));
        Some(server)
    }

    async fn start(&self) -> Option<(Helper, AppServer)> {
        let codex = self.codex.as_ref()?;
        let runtime = self.runtime.as_ref()?;
        std::fs::create_dir_all(runtime).ok()?;
        std::fs::set_permissions(runtime, std::fs::Permissions::from_mode(0o700)).ok()?;
        if !claude_sessions::private_directory(runtime) {
            return None;
        }
        let rendezvous = runtime.join("codex-app-server.sock");
        let trusted_before = trusts_directory(&self.home, runtime);
        let mut command = Command::new(codex);
        command
            .arg("app-server")
            .arg("--listen")
            .arg(format!("unix://{}", rendezvous.display()))
            // A fixed, empty working directory: no project's configuration applies.
            .current_dir(runtime)
            .env("CODEX_HOME", &self.home)
            .stdin(std::process::Stdio::null())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            // Its own group, so a launcher and the server it runs stop together.
            .process_group(0)
            .kill_on_drop(true);
        process_lifetime::kill_with_parent(&mut command);
        let mut child = command.spawn().ok()?;
        let pid = child.id()?;
        let deadline = Instant::now() + HELPER_START;
        let server = loop {
            if let Some(socket) = rendezvous_target(&rendezvous)
                && let Ok(server) = AppServer::connect(&socket, Peer::Within(pid)).await
            {
                break server;
            }
            if Instant::now() >= deadline || !matches!(child.try_wait(), Ok(None)) {
                kill_group(pid);
                let _ = child.kill().await;
                return None;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        };
        let helper = Helper {
            child,
            pid,
            rendezvous,
            used: Instant::now(),
            trusted_before,
        };
        report_trust(&helper, &self.home);
        Some((helper, server))
    }

    /// Count a delivery in flight until the returned guard is dropped.
    pub fn delivering(&self) -> InFlight<'_> {
        self.deliveries.fetch_add(1, Ordering::AcqRel);
        InFlight(self)
    }

    /// Mluva is closing: start no helper, let deliveries in flight finish (a Codex message being
    /// withdrawn included), within a bound, then stop the helper.
    pub async fn shutdown(&self) {
        self.closing.store(true, Ordering::Release);
        let deadline = tokio::time::Instant::now() + SHUTDOWN_DRAIN;
        loop {
            let drained = self.drained.notified();
            if self.deliveries.load(Ordering::Acquire) == 0 {
                break;
            }
            if tokio::time::timeout_at(deadline, drained).await.is_err() {
                break;
            }
        }
        if let Some(helper) = self.helper.lock().await.take() {
            stop(helper, &self.home).await;
        }
    }

    /// Whether the helper process is running; for tests and diagnostics.
    pub async fn helper_running(&self) -> bool {
        self.helper
            .lock()
            .await
            .as_mut()
            .is_some_and(|helper| matches!(helper.child.try_wait(), Ok(None)))
    }

    /// The helper's process, while it runs; for tests and diagnostics.
    pub async fn helper_pid(&self) -> Option<u32> {
        self.helper.lock().await.as_ref().map(|helper| helper.pid)
    }
}

/// Stop the helper once nobody has used it for `idle`, whatever else Mluva is doing.
async fn watchdog(slot: Arc<Mutex<Option<Helper>>>, idle: Duration, home: PathBuf) {
    loop {
        tokio::time::sleep(idle.min(IDLE_CHECK).max(Duration::from_millis(10))).await;
        let mut helper = slot.lock().await;
        let Some(running) = helper.as_mut() else {
            return;
        };
        let exited = !matches!(running.child.try_wait(), Ok(None));
        if exited || running.used.elapsed() >= idle {
            if let Some(stopped) = helper.take() {
                stop(stopped, &home).await;
            }
            return;
        }
    }
}

/// Stop the helper's whole process group: politely, then for good.
async fn stop(mut helper: Helper, home: &Path) {
    report_trust(&helper, home);
    // SAFETY: signals only the process group Mluva created for its helper.
    unsafe { libc::killpg(helper.pid as libc::pid_t, libc::SIGTERM) };
    if tokio::time::timeout(Duration::from_secs(1), helper.child.wait())
        .await
        .is_err()
    {
        kill_group(helper.pid);
        let _ = helper.child.kill().await;
    }
    kill_group(helper.pid);
}

fn kill_group(pid: u32) {
    // SAFETY: signals only the process group Mluva created for its helper.
    unsafe { libc::killpg(pid as libc::pid_t, libc::SIGKILL) };
}

/// Report, as a fact only, a trust entry for the helper's own working directory that was not
/// there before it started: the one configuration change attributable to the helper.
fn report_trust(helper: &Helper, home: &Path) {
    let Some(directory) = helper.rendezvous.parent() else {
        return;
    };
    if !helper.trusted_before && trusts_directory(home, directory) {
        eprintln!("Codex recorded Mluva's Codex app server directory as a trusted project.");
    }
}

/// Whether Codex's configuration has a `[projects."<directory>"]` entry.
fn trusts_directory(home: &Path, directory: &Path) -> bool {
    let Ok(file) = std::fs::File::open(home.join("config.toml")) else {
        return false;
    };
    let mut text = String::new();
    if file.take(MAX_CONFIG).read_to_string(&mut text).is_err() {
        return false;
    }
    let directory = directory.to_string_lossy();
    text.lines().any(|line| {
        let line = line.trim();
        line.starts_with("[projects.") && line.contains(directory.as_ref())
    })
}

/// The socket a Codex rendezvous path leads to: itself when it is a socket, else the symlink's
/// target in `codex-daemon-<uid>`. Either way the socket and its directory must be owned by
/// this account and the directory closed to everyone else.
pub(crate) fn rendezvous_target(path: &Path) -> Option<PathBuf> {
    let uid = agent_platform::effective_uid();
    let metadata = std::fs::symlink_metadata(path).ok()?;
    let socket = if metadata.file_type().is_symlink() {
        let target = std::fs::read_link(path).ok()?;
        let target = if target.is_absolute() {
            target
        } else {
            path.parent()?.join(target)
        };
        let directory = target.parent()?;
        let name = directory.file_name()?.to_str()?;
        if name != format!("codex-daemon-{uid}") {
            return None;
        }
        target
    } else {
        path.to_path_buf()
    };
    let directory = std::fs::symlink_metadata(socket.parent()?).ok()?;
    let file = std::fs::symlink_metadata(&socket).ok()?;
    (directory.file_type().is_dir()
        && directory.uid() == uid
        && directory.mode() & 0o077 == 0
        && file.file_type().is_socket()
        && file.uid() == uid)
        .then_some(socket)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_private_socket_or_a_link_into_this_accounts_daemon_directory_is_trusted() {
        let temp = crate::test_sockets::socket_dir();
        let uid = agent_platform::effective_uid();
        let daemon = temp.path().join(format!("codex-daemon-{uid}"));
        std::fs::create_dir(&daemon).unwrap();
        std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o700)).unwrap();
        let socket = daemon.join("0123abcd");
        let _listener = std::os::unix::net::UnixListener::bind(&socket).unwrap();
        let link = temp.path().join("app-server-control.sock");
        std::os::unix::fs::symlink(&socket, &link).unwrap();
        assert_eq!(rendezvous_target(&link), Some(socket.clone()));
        assert_eq!(rendezvous_target(&socket), Some(socket.clone()));
        // A dangling link means no server.
        let dangling = temp.path().join("dangling.sock");
        std::os::unix::fs::symlink(daemon.join("gone"), &dangling).unwrap();
        assert_eq!(rendezvous_target(&dangling), None);
        // A directory others can enter, or a link elsewhere, is not trusted.
        std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o755)).unwrap();
        assert_eq!(rendezvous_target(&link), None);
        std::fs::set_permissions(&daemon, std::fs::Permissions::from_mode(0o700)).unwrap();
        let other = temp.path().join("elsewhere");
        std::fs::create_dir(&other).unwrap();
        std::fs::set_permissions(&other, std::fs::Permissions::from_mode(0o700)).unwrap();
        let foreign = other.join("socket");
        let _other = std::os::unix::net::UnixListener::bind(&foreign).unwrap();
        let misdirected = temp.path().join("misdirected.sock");
        std::os::unix::fs::symlink(&foreign, &misdirected).unwrap();
        assert_eq!(rendezvous_target(&misdirected), None);
    }

    #[test]
    fn a_stale_or_reused_daemon_pid_file_names_no_daemon() {
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex");
        let directory = home.join("app-server-daemon");
        std::fs::create_dir_all(&directory).unwrap();
        let servers = Servers::new(home.clone(), None, None);
        let own = std::process::id();
        let write = |boot: &str, ticks: &str| {
            let path = directory.join("daemon.pid");
            std::fs::write(
                &path,
                format!(
                    r#"{{"pid":{own},"processStartTime":"x","processIdentity":{{"bootId":"{boot}","startTicks":{ticks}}}}}"#
                ),
            )
            .unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o600)).unwrap();
        };
        let boot = agent_platform::boot_id().unwrap();
        let ticks = agent_platform::process_start(own).unwrap();
        // From before the last reboot.
        write("00000000-0000-4000-8000-000000000000", &ticks);
        assert!(servers.daemon().is_none());
        // The PID now belongs to a process started at another time.
        write(&boot, "1");
        assert!(servers.daemon().is_none());
        // This boot and this process, but no control socket: no daemon either.
        write(&boot, &ticks);
        assert!(servers.daemon().is_none());
    }

    /// The real `codex app-server`, started as Mluva's helper in a private `CODEX_HOME` whose
    /// configuration has the keys that matter in a person's real one, leaves that configuration
    /// byte-identical after it starts and answers Mluva's own calls.
    #[tokio::test]
    #[ignore = "runs the installed codex executable in a private CODEX_HOME"]
    async fn the_helper_leaves_a_real_shaped_configuration_byte_identical() {
        let codex = agent_platform::codex_executable().expect("codex installed");
        let temp = tempfile::tempdir().unwrap();
        let home = temp.path().join("codex-home");
        std::fs::create_dir_all(&home).unwrap();
        let config = r#"approvals_reviewer = "user"
model = "synthetic-model"
approval_policy = "never"
sandbox_mode = "danger-full-access"
check_for_update_on_startup = false

[projects."/synthetic/project"]
trust_level = "trusted"

[tui]
vim_mode_default = true

[desktop]
followUpQueueMode = "steer"

[features]
hooks = true

[hooks.state."/synthetic/hooks.json:session_start:0:0"]
trusted_hash = "0000000000000000000000000000000000000000000000000000000000000000"
"#;
        std::fs::write(home.join("config.toml"), config).unwrap();
        let runtime = temp.path().join("runtime");
        let servers = Servers::new(home.clone(), Some(runtime), Some(codex));
        let (mut server, owner) = servers.connect().await.expect("helper answers");
        assert_eq!(owner, Owner::Helper);
        let helper = servers.helper_pid().await.unwrap();
        let unknown = "01900000-0000-7000-8000-00000000dead";
        for (method, params) in [
            ("thread/loaded/list", serde_json::json!({})),
            (
                "thread/read",
                serde_json::json!({"threadId": unknown, "includeTurns": false}),
            ),
            (
                "thread/queue/list",
                serde_json::json!({"threadId": unknown}),
            ),
        ] {
            let _ = server.call(method, params).await;
        }
        drop(server);
        assert!(servers.helper_running().await);
        servers.shutdown().await;
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
        // The whole group is gone.
        assert!(!std::path::Path::new(&format!("/proc/{helper}")).exists());
    }
}
