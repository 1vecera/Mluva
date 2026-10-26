//! Which Codex app server Mluva talks to.
//!
//! Every app server on the same `CODEX_HOME` shares Codex's queue store, so any of them can
//! queue a message for a thread another process owns. Mluva uses the shared daemon's control
//! socket when the daemon runs and answers. Otherwise it starts its own `codex app-server`,
//! listening only on a socket in Mluva's private runtime directory, as a child process: no
//! installed service and never a managed daemon. It is started when needed, stopped after
//! being idle and when Mluva exits, and offered to no other client, so it hosts nobody's
//! threads.
//!
//! An app server writes `config.toml` only when asked to: starting a thread with full access
//! records the project as trusted, and the configuration methods write what they are given.
//! Mluva never calls those. As a guard all the same, the configuration is fingerprinted before
//! the helper starts and after each use; if it changed, Mluva stops the helper and does not
//! start it again for the rest of its run.
use super::rpc::AppServer;
use crate::{agent_platform, claude_sessions, process_lifetime};
use serde_json::Value;
use sha2::{Digest, Sha256};
use std::{
    io::Read,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    sync::{
        OnceLock,
        atomic::{AtomicBool, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::{process::Command, sync::Mutex};

/// A helper nobody used for this long is stopped.
const HELPER_IDLE: Duration = Duration::from_secs(10 * 60);
/// How long a starting helper may take to answer.
const HELPER_START: Duration = Duration::from_secs(15);
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
    helper: Mutex<Option<Helper>>,
    /// The helper changed Codex's configuration once; it is not started again.
    disabled: AtomicBool,
    /// A test's own app server, served by the test process, standing in for the daemon.
    #[cfg(test)]
    fixed: Option<PathBuf>,
}

struct Helper {
    child: tokio::process::Child,
    pid: u32,
    rendezvous: PathBuf,
    used: Instant,
    config: Option<[u8; 32]>,
}

impl Servers {
    pub fn new(home: PathBuf, runtime: Option<PathBuf>, codex: Option<PathBuf>) -> Self {
        Self {
            home,
            runtime,
            codex,
            helper: Mutex::new(None),
            disabled: AtomicBool::new(false),
            #[cfg(test)]
            fixed: None,
        }
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
        if let Some(server) = self.connect_daemon().await {
            return Some((server, Owner::Daemon));
        }
        self.helper().await.map(|server| (server, Owner::Helper))
    }

    /// The daemon alone, when it runs and answers; it never starts anything.
    pub async fn connect_daemon(&self) -> Option<AppServer> {
        #[cfg(test)]
        if let Some(socket) = &self.fixed {
            return AppServer::connect(socket, Some(std::process::id()))
                .await
                .ok();
        }
        let (socket, pid) = self.daemon()?;
        AppServer::connect(&socket, Some(pid)).await.ok()
    }

    /// The daemon's socket and process, when its PID file names a live process from this boot
    /// and its control socket is private to this account.
    fn daemon(&self) -> Option<(PathBuf, u32)> {
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
        if identity.get("bootId")?.as_str()? != agent_platform::boot_id()?
            || identity.get("startTicks")?.as_u64()?.to_string()
                != agent_platform::process_start(pid)?
        {
            return None;
        }
        let socket =
            rendezvous_target(&self.home.join("app-server-control/app-server-control.sock"))?;
        Some((socket, pid))
    }

    /// Mluva's own helper, started if it is not running.
    async fn helper(&self) -> Option<AppServer> {
        if self.disabled.load(Ordering::Relaxed) {
            return None;
        }
        let mut helper = self.helper.lock().await;
        if let Some(running) = helper.as_mut() {
            let alive = matches!(running.child.try_wait(), Ok(None));
            if alive && let Some(socket) = rendezvous_target(&running.rendezvous) {
                running.used = Instant::now();
                if let Ok(server) = AppServer::connect(&socket, Some(running.pid)).await {
                    return Some(server);
                }
            }
            *helper = None;
        }
        let (started, server) = self.start().await?;
        *helper = Some(started);
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
        let config = fingerprint(&self.home.join("config.toml"));
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
            .kill_on_drop(true);
        process_lifetime::kill_with_parent(&mut command);
        let mut child = command.spawn().ok()?;
        let pid = child.id()?;
        let deadline = Instant::now() + HELPER_START;
        let server = loop {
            if let Some(socket) = rendezvous_target(&rendezvous)
                && let Ok(server) = AppServer::connect(&socket, Some(pid)).await
            {
                break server;
            }
            if Instant::now() >= deadline || !matches!(child.try_wait(), Ok(None)) {
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
            config,
        };
        if !self.unchanged(&helper) {
            self.stop_changed(helper).await;
            return None;
        }
        Some((helper, server))
    }

    /// Whether Codex's configuration is as it was before the helper started.
    fn unchanged(&self, helper: &Helper) -> bool {
        fingerprint(&self.home.join("config.toml")) == helper.config
    }

    async fn stop_changed(&self, mut helper: Helper) {
        let _ = helper.child.kill().await;
        self.disabled.store(true, Ordering::Relaxed);
        // Only the fact: no path, value or content.
        eprintln!(
            "Codex's configuration changed while Mluva's Codex app server ran; Mluva stopped it."
        );
    }

    /// After a use: stop the helper if the configuration changed, or if it sat idle too long.
    pub async fn tidy(&self) {
        let mut helper = self.helper.lock().await;
        let Some(running) = helper.as_ref() else {
            return;
        };
        if !self.unchanged(running) {
            if let Some(changed) = helper.take() {
                self.stop_changed(changed).await;
            }
        } else if running.used.elapsed() > HELPER_IDLE
            && let Some(mut idle) = helper.take()
        {
            let _ = idle.child.kill().await;
        }
    }

    /// Stop the helper now, as Mluva closes.
    pub async fn shutdown(&self) {
        if let Some(mut helper) = self.helper.lock().await.take() {
            let _ = helper.child.kill().await;
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

/// A digest of Codex's configuration, or `None` when there is none.
fn fingerprint(path: &Path) -> Option<[u8; 32]> {
    let file = std::fs::File::open(path).ok()?;
    let mut bytes = Vec::new();
    file.take(MAX_CONFIG).read_to_end(&mut bytes).ok()?;
    Some(Sha256::digest(&bytes).into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_private_socket_or_a_link_into_this_accounts_daemon_directory_is_trusted() {
        let temp = tempfile::tempdir().unwrap();
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
        servers.tidy().await;
        assert!(servers.helper_running().await, "the guard saw no change");
        servers.shutdown().await;
        assert_eq!(
            std::fs::read_to_string(home.join("config.toml")).unwrap(),
            config
        );
    }
}
