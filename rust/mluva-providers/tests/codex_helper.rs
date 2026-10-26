//! Mluva's own Codex app server as real child processes: a launcher that runs the server as its
//! child, the idle watchdog and closing. The server is the Codex fixture peer.
use mluva_providers::codex::server::{Owner, Servers};
use serde_json::json;
use std::{
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    sync::Arc,
    time::{Duration, Instant},
};

/// A private home and runtime directory, and a launcher shaped like `npm`'s `codex`: a script
/// that starts the real server as its own child and waits for it.
struct Fixture {
    temp: tempfile::TempDir,
    launcher: PathBuf,
}

impl Fixture {
    fn new() -> Self {
        let temp = tempfile::tempdir().unwrap();
        std::fs::create_dir(temp.path().join("codex-home")).unwrap();
        let launcher = temp.path().join("codex");
        std::fs::write(
            &launcher,
            format!(
                "#!/bin/sh\n\"{}\" \"$@\" &\nwait\n",
                env!("CARGO_BIN_EXE_codex-app-server-fixture-peer")
            ),
        )
        .unwrap();
        std::fs::set_permissions(&launcher, std::fs::Permissions::from_mode(0o700)).unwrap();
        Self { temp, launcher }
    }
    fn servers(&self, idle: Duration) -> Servers {
        Servers::new(
            self.temp.path().join("codex-home"),
            Some(self.temp.path().join("runtime")),
            Some(self.launcher.clone()),
        )
        .with_idle(idle)
    }
}

/// The children of a process, from `/proc`.
fn children(pid: u32) -> Vec<u32> {
    std::fs::read_dir("/proc")
        .unwrap()
        .flatten()
        .filter_map(|entry| entry.file_name().to_str()?.parse::<u32>().ok())
        .filter(|candidate| {
            std::fs::read_to_string(format!("/proc/{candidate}/stat")).is_ok_and(|stat| {
                stat[stat.rfind(')').unwrap() + 1..]
                    .split_whitespace()
                    .nth(1)
                    .and_then(|parent| parent.parse::<u32>().ok())
                    == Some(pid)
            })
        })
        .collect()
}

/// Gone, or left only as a zombie for its new parent to reap.
fn ended(pid: u32) -> bool {
    std::fs::read_to_string(format!("/proc/{pid}/stat")).map_or(true, |stat| {
        stat[stat.rfind(')').unwrap() + 1..]
            .trim_start()
            .starts_with('Z')
    })
}

async fn until(what: &str, mut condition: impl FnMut() -> bool) {
    let deadline = Instant::now() + Duration::from_secs(5);
    while !condition() {
        assert!(Instant::now() < deadline, "{what}");
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn a_server_run_by_a_launcher_is_accepted_and_stopped_with_it_when_idle() {
    let fixture = Fixture::new();
    let servers = fixture.servers(Duration::from_millis(400));
    let (mut server, owner) = servers.connect().await.expect("the helper answers");
    assert_eq!(owner, Owner::Helper);
    assert!(server.call("thread/loaded/list", json!({})).await.is_ok());
    let launcher = servers.helper_pid().await.unwrap();
    let real = children(launcher);
    assert_eq!(real.len(), 1, "the launcher runs the server as its child");
    drop(server);
    // Nobody uses it: its own watchdog stops the launcher and the server, with no discovery.
    until("the idle helper stops", || {
        !Path::new(&format!("/proc/{launcher}")).exists() || ended(launcher)
    })
    .await;
    until("the server stops with its launcher", || ended(real[0])).await;
    assert!(!servers.helper_running().await);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn closing_waits_for_a_delivery_in_flight_and_then_starts_no_helper() {
    let fixture = Fixture::new();
    let servers = Arc::new(fixture.servers(Duration::from_secs(600)));
    let (server, _) = servers.connect().await.expect("the helper answers");
    let launcher = servers.helper_pid().await.unwrap();
    let in_flight = servers.delivering();
    let closing = {
        let servers = servers.clone();
        tokio::spawn(async move { servers.shutdown().await })
    };
    tokio::time::sleep(Duration::from_millis(400)).await;
    assert!(
        !closing.is_finished(),
        "closing waits while a delivery may be withdrawing"
    );
    assert!(
        servers.helper_running().await,
        "the helper stays for the withdrawal"
    );
    drop(server);
    drop(in_flight);
    tokio::time::timeout(Duration::from_secs(3), closing)
        .await
        .expect("closing finishes once the delivery has")
        .unwrap();
    until("the helper stops", || ended(launcher)).await;
    assert!(
        servers.connect().await.is_none(),
        "no helper starts while Mluva closes"
    );
    assert!(servers.helper_pid().await.is_none());
}
