//! Development-only external client. Build as an example; never installed with Mluva.
#[path = "verify/accessibility.rs"]
mod accessibility;
#[path = "verify/fixture.rs"]
mod fixture;
#[path = "verify/isolation.rs"]
mod isolation;
#[path = "verify/performance.rs"]
mod performance;

use glib::variant::ToVariant;
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    fs,
    io::Write,
    path::{Path, PathBuf},
    process::{Command, Stdio},
    thread,
    time::{Duration, Instant},
};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;
const APP: &str = "com.mluva.Linux";
const APP_PATH: &str = "/com/mluva/Linux";

fn require(condition: bool, message: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(message.into())
    }
}
fn path_env(name: &str) -> Result<PathBuf> {
    Ok(PathBuf::from(std::env::var_os(name).ok_or_else(|| {
        format!("missing {name}; use dev/verify.sh")
    })?))
}
fn connection(address: &str) -> Result<gio::DBusConnection> {
    Ok(gio::DBusConnection::for_address_sync(
        address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        gio::Cancellable::NONE,
    )?)
}
fn session_bus() -> Result<gio::DBusConnection> {
    connection(&std::env::var("DBUS_SESSION_BUS_ADDRESS")?)
}
fn call(
    bus: &gio::DBusConnection,
    name: &str,
    path: &str,
    interface: &str,
    method: &str,
    params: Option<&glib::Variant>,
) -> Result<glib::Variant> {
    Ok(bus.call_sync(
        Some(name),
        path,
        interface,
        method,
        params,
        None,
        gio::DBusCallFlags::NO_AUTO_START,
        1500,
        gio::Cancellable::NONE,
    )?)
}
fn bus_pid(bus: &gio::DBusConnection, name: &str) -> Result<u32> {
    Ok(call(
        bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetConnectionUnixProcessID",
        Some(&(name,).to_variant()),
    )?
    .child_get::<u32>(0))
}
fn process_identity(pid: u32) -> Result<(String, String)> {
    let stat = fs::read_to_string(format!("/proc/{pid}/stat"))?;
    let fields = stat
        .rsplit_once(") ")
        .ok_or("invalid process stat")?
        .1
        .split_whitespace()
        .collect::<Vec<_>>();
    Ok((
        fields[0].into(),
        fields
            .get(19)
            .ok_or("missing process start time")?
            .to_string(),
    ))
}
fn exited(pid: u32, start: &str) -> bool {
    match process_identity(pid) {
        Ok((state, actual_start)) => state == "Z" || actual_start != start,
        Err(_) => !Path::new(&format!("/proc/{pid}")).exists(),
    }
}
fn app_owner() -> Result<(String, u32)> {
    let bus = session_bus()?;
    let owner = call(
        &bus,
        "org.freedesktop.DBus",
        "/org/freedesktop/DBus",
        "org.freedesktop.DBus",
        "GetNameOwner",
        Some(&(APP,).to_variant()),
    )?
    .child_get::<String>(0);
    let pid = bus_pid(&bus, &owner)?;
    let root = path_env("OFFSCREEN_SESSION_ROOT")?;
    let identity: Value = serde_json::from_slice(&fs::read(root.join("app-process.json"))?)?;
    require(
        identity["pid"] == pid && identity["start"] == process_identity(pid)?.1,
        "application PID/start identity differs from the spawned process",
    )?;
    require(
        fs::read_link(format!("/proc/{pid}/exe"))? == root.join("bundle/bin/mluva"),
        "D-Bus owner is not this run's Mluva executable",
    )?;
    let environ = fs::read(format!("/proc/{pid}/environ"))?;
    for key in [
        "HOME",
        "XDG_CONFIG_HOME",
        "XDG_DATA_HOME",
        "DISPLAY",
        "AT_SPI_BUS_ADDRESS",
    ] {
        let expected = format!("{key}={}", std::env::var(key)?);
        require(
            environ.split(|b| *b == 0).any(|s| s == expected.as_bytes()),
            &format!("Mluva inherited unexpected {key}"),
        )?;
    }
    Ok((owner, pid))
}
fn actions() -> Result<Value> {
    let (owner, _) = app_owner()?;
    Ok(json!(
        call(
            &session_bus()?,
            &owner,
            APP_PATH,
            "org.gtk.Actions",
            "List",
            None
        )?
        .child_value(0)
        .get::<Vec<String>>()
        .ok_or("invalid action list")?
    ))
}
fn action(name: &str) -> Result<Value> {
    require(
        ["record", "cancel", "latest", "history", "settings", "quit"].contains(&name),
        "supported actions: record, cancel, latest, history, settings, quit",
    )?;
    require(
        actions()?.as_array().unwrap().iter().any(|v| v == name),
        "requested production action is absent",
    )?;
    let (owner, pid) = app_owner()?;
    let (_, start) = process_identity(pid)?;
    call(
        &session_bus()?,
        &owner,
        APP_PATH,
        "org.gtk.Actions",
        "Activate",
        Some(
            &(
                name,
                Vec::<glib::Variant>::new(),
                BTreeMap::<String, glib::Variant>::new(),
            )
                .to_variant(),
        ),
    )?;
    if name == "quit" {
        until("normal Quit", || {
            let reply = call(
                &session_bus()?,
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "NameHasOwner",
                Some(&(APP,).to_variant()),
            )?;
            require(
                !reply.child_get::<bool>(0),
                "Mluva still owns its application name",
            )?;
            require(
                exited(pid, &start),
                "Mluva dropped its bus name but has not exited",
            )
        })?;
        // Reap when this client is also the parent; another session client may
        // observe an already-reaped child instead. Never wait on an unrelated PID.
        unsafe {
            libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG);
        }
    }
    Ok(
        json!({"action":name,"transport":"production org.gtk.Actions","owner":owner,"pid":pid,"start":start,"process_exited":name == "quit"}),
    )
}
fn launch(global_shortcuts: bool) -> Result<Value> {
    let bus = session_bus()?;
    require(
        !call(
            &bus,
            "org.freedesktop.DBus",
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(APP,).to_variant()),
        )?
        .child_get::<bool>(0),
        "Mluva already running; Quit before launch",
    )?;
    let root = path_env("OFFSCREEN_SESSION_ROOT")?;
    if let Ok(bytes) = fs::read(root.join("app-process.json")) {
        let previous: Value = serde_json::from_slice(&bytes)?;
        require(
            exited(
                previous["pid"].as_u64().ok_or("invalid saved PID")? as u32,
                previous["start"].as_str().ok_or("invalid saved start")?,
            ),
            "previous app has not exited",
        )?;
    }
    let log = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(root.join("app.log"))?;
    let mut command = Command::new(root.join("bundle/bin/mluva"));
    if global_shortcuts {
        bus_pid(&bus, "org.freedesktop.portal.Desktop")?;
        command.env_remove("MLUVA_DISABLE_GLOBAL_SHORTCUT");
    } else {
        command.env("MLUVA_DISABLE_GLOBAL_SHORTCUT", "1");
    }
    let mut child = command
        .env("GSETTINGS_BACKEND", "memory")
        .env(
            "PATH",
            format!(
                "{}:{}",
                root.join("tools").display(),
                std::env::var("PATH")?
            ),
        )
        .stdin(Stdio::null())
        .stdout(log.try_clone()?)
        .stderr(log)
        .spawn()?;
    let (_, start) = process_identity(child.id())?;
    fs::write(
        root.join("app-process.json"),
        serde_json::to_vec(&json!({"pid":child.id(),"start":start}))?,
    )?;
    let ready = until("application startup", || {
        require(
            child.try_wait()?.is_none(),
            "Mluva exited during startup; inspect app.log",
        )?;
        let (_, pid) = app_owner()?;
        require(
            pid == child.id(),
            "startup activated an unexpected instance",
        )?;
        require(
            actions()?.as_array().unwrap().iter().any(|a| a == "record"),
            "record action not ready",
        )?;
        accessibility::nodes()?;
        Ok(())
    });
    if ready.is_err() {
        let _ = child.kill();
        let _ = child.wait();
    }
    ready?;
    Ok(json!({"pid":child.id(),"start":start,"executable":root.join("bundle/bin/mluva")}))
}
fn stored() -> Result<Value> {
    let file = path_env("XDG_DATA_HOME")?.join("mluva/history.sqlite3");
    let db =
        rusqlite::Connection::open_with_flags(file, rusqlite::OpenFlags::SQLITE_OPEN_READ_ONLY)?;
    let mut query = db.prepare("SELECT h.identifier, h.raw_text, COALESCE(s.text, h.raw_text), h.delivered_text, h.delivery_outcome, h.retained_audio_path FROM transcription_history h LEFT JOIN conversation_sources s ON s.history_identifier = h.identifier ORDER BY h.created_at DESC, h.rowid DESC LIMIT 20")?;
    let rows = query.query_map([], |row| Ok(json!({"id":row.get::<_,String>(0)?,"raw":row.get::<_,String>(1)?,"working":row.get::<_,String>(2)?,"delivered":row.get::<_,String>(3)?,"delivery_outcome":row.get::<_,String>(4)?,"retained_audio":row.get::<_,Option<String>>(5)?})))?.collect::<std::result::Result<Vec<_>,_>>()?;
    Ok(json!({"count":rows.len(),"history":rows}))
}
fn state() -> Result<Value> {
    let mut value = stored()?;
    let (owner, pid) = app_owner()?;
    value["owner"] = json!(owner);
    value["pid"] = json!(pid);
    let tree = accessibility::nodes()?;
    value["editor"] = match accessibility::editor(&tree) {
        Ok(node) => node["text"].clone(),
        Err(error) => {
            value["editor_unavailable"] = json!(error.to_string());
            Value::Null
        }
    };
    Ok(value)
}
fn until(label: &str, mut check: impl FnMut() -> Result<()>) -> Result<()> {
    let deadline = Instant::now() + Duration::from_secs(12);
    loop {
        match check() {
            Ok(()) => return Ok(()),
            Err(error) if Instant::now() >= deadline => {
                return Err(format!("{label} timed out: {error}").into());
            }
            Err(_) => thread::sleep(Duration::from_millis(100)),
        }
    }
}
fn assertion(field: &str, expected: &str) -> Result<Value> {
    require(
        ["raw", "working", "editor", "count"].contains(&field),
        "assert field must be raw, working, editor or count",
    )?;
    until(&format!("assert {field}"), || {
        let observed = if field == "editor" {
            state()?["editor"].clone()
        } else if field == "count" {
            stored()?["count"].clone()
        } else {
            stored()?["history"][0][field].clone()
        };
        let actual = observed
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| observed.to_string());
        require(
            actual == expected,
            &format!(
                "expected {expected:?}, observed {actual:?}; inspect state/discover and app.log"
            ),
        )
    })?;
    Ok(json!({"field":field,"expected":expected}))
}
fn screenshot(name: &str) -> Result<Value> {
    require(
        !name.is_empty()
            && name.len() <= 80
            && name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_'),
        "screenshot name must contain only letters, digits, '-' or '_'",
    )?;
    let path = path_env("OFFSCREEN_SESSION_ROOT")?.join(format!("{name}.png"));
    require(
        Command::new("timeout")
            .args(["8", "import", "-window", "root"])
            .arg(&path)
            .status()?
            .success(),
        "private screenshot failed",
    )?;
    let info = Command::new("identify")
        .args(["-format", "%w %h %[fx:standard_deviation]"])
        .arg(&path)
        .output()?;
    require(info.status.success(), "screenshot inspection failed")?;
    let metrics = String::from_utf8(info.stdout)?;
    let fields = metrics.split_whitespace().collect::<Vec<_>>();
    require(
        fields.len() == 3
            && fields[0].parse::<u32>()? >= 800
            && fields[1].parse::<u32>()? >= 600
            && fields[2].parse::<f64>()? > 0.01,
        "screenshot is blank or too small",
    )?;
    Ok(json!({"path":path,"metrics":metrics}))
}
fn dispatch(args: &[String]) -> Result<Value> {
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["doctor"] => { let (owner,pid) = app_owner()?; Ok(json!({"owner":owner,"pid":pid,"session":isolation::check()?,"actions":actions()?})) }
        ["discover"] => Ok(json!({"actions":actions()?,"nodes":accessibility::nodes()?})),
        ["state"] => state(),
        ["launch"] => launch(false),
        ["launch", "--global-shortcuts"] => launch(true),
        ["click", name] => accessibility::click(name),
        ["action", name] => action(name),
        ["edit", text] => accessibility::edit(text),
        ["assert", field, expected] => assertion(field, expected),
        ["screenshot", name] => screenshot(name),
        _ => Err("usage: doctor|discover|state|launch [--global-shortcuts]|click BUTTON|action NAME|edit TEXT|assert FIELD EXPECTED|screenshot NAME".into())
    }
}
fn record(args: &[String]) -> Result<Value> {
    let result = dispatch(args);
    let mut line = match &result {
        Ok(data) => json!({"ok":true,"command":args,"data":data}),
        Err(error) => json!({"ok":false,"command":args,"error":error.to_string()}),
    };
    line["at"] = json!(chrono::Utc::now().to_rfc3339());
    let root = path_env("OFFSCREEN_SESSION_ROOT")?;
    writeln!(
        fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(root.join("trace.jsonl"))?,
        "{line}"
    )?;
    result.map(|_| line)
}
fn step(args: &[&str]) -> Result<Value> {
    record(&args.iter().map(|s| (*s).into()).collect::<Vec<_>>())
}
fn smoke(expected: &str, root: &Path) -> Result<()> {
    step(&["doctor"])?;
    step(&["discover"])?;
    step(&["screenshot", "startup"])?;
    step(&["action", "record"])?;
    until("synthetic recording", || {
        require(
            root.join("tools/raw.ready.json").exists(),
            "pw-record fixture has not emitted PCM",
        )
    })?;
    step(&["screenshot", "recording"])?;
    step(&["action", "record"])?;
    step(&["assert", "count", "1"])?;
    step(&["assert", "raw", fixture::RAW])?;
    step(&["assert", "editor", fixture::RAW])?;
    let initial = stored()?["history"][0].clone();
    let clipboard = private_clipboard()?;
    require(
        initial["delivered"] == fixture::RAW && clipboard == fixture::RAW.as_bytes(),
        "manual dictation did not copy/deliver the original",
    )?;
    step(&["edit", fixture::EDIT])?;
    step(&["assert", "working", fixture::EDIT])?;
    step(&["assert", "raw", fixture::RAW])?;
    require(
        private_clipboard()? == clipboard,
        "autosave changed the private clipboard",
    )?;
    step(&["screenshot", "edited"])?;
    step(&["state"])?;
    step(&["action", "quit"])?;
    step(&["launch"])?;
    step(&["action", "latest"])?;
    step(&["assert", "editor", fixture::EDIT])?;
    step(&["assert", "working", expected])?;
    step(&["assert", "raw", fixture::RAW])?;
    step(&["screenshot", "reopened"])?;
    step(&["state"])?;
    let final_entry = stored()?["history"][0].clone();
    require(
        final_entry["id"] == initial["id"]
            && final_entry["raw"] == fixture::RAW
            && final_entry["delivered"] == initial["delivered"],
        "editing/reopen changed the original identity or delivered text",
    )?;
    require(
        final_entry["retained_audio"].is_null(),
        "retention Never left retained audio metadata",
    )?;
    let recordings = path_env("XDG_DATA_HOME")?.join("mluva/recordings");
    require(
        !recordings.exists() || fs::read_dir(recordings)?.next().is_none(),
        "retention Never left recording files",
    )?;
    fs::write(
        root.join("invariants.json"),
        serde_json::to_vec_pretty(
            &json!({"original_unchanged":true,"delivered_unchanged":true,"conversation_identity_unchanged":true,"clipboard_unchanged_by_edit":true,"retained_audio_empty":true,"initial":initial,"final":final_entry}),
        )?,
    )?;
    Ok(())
}
fn private_clipboard() -> Result<Vec<u8>> {
    let output = Command::new("timeout")
        .args(["3", "xclip", "-selection", "clipboard", "-o"])
        .output()?;
    require(
        output.status.success(),
        "private clipboard could not be read",
    )?;
    Ok(output.stdout)
}
fn run(args: &[String], root: &Path) -> Result<Value> {
    require(args.len() >= 3, "run requires mode, checkout, binaries")?;
    let mode = &args[0];
    if mode == "performance" {
        return performance::run(root, Path::new(&args[1]), Path::new(&args[2]));
    }
    let mut peer = fixture::prepare(root, Path::new(&args[1]), Path::new(&args[2]))?;
    let outcome = (|| {
        step(&["launch"])?;
        if mode == "smoke" {
            let expected = match &args[3..] {
                [] => fixture::EDIT,
                [flag, value] if flag == "--expect-working" => value,
                _ => return Err("smoke accepts only --expect-working TEXT".into()),
            };
            smoke(expected, root)?;
            fixture::check_requests(&peer, root)?;
        } else {
            require(
                mode == "session" && args.len() >= 4,
                "session requires a command",
            )?;
            let status = Command::new(&args[3])
                .args(&args[4..])
                .env("MLUVA_VERIFY_CLI", std::env::current_exe()?)
                .status()?;
            require(
                status.success(),
                &format!("session command exited {status}"),
            )?;
        }
        Ok::<_, Box<dyn std::error::Error>>(())
    })();
    if outcome.is_err() {
        let _ = screenshot("failure");
        let diagnostic = json!({"state":state().map_err(|e|e.to_string()),"discovery":accessibility::nodes().map_err(|e|e.to_string())});
        fs::write(
            root.join("failure-state.json"),
            serde_json::to_vec_pretty(&diagnostic)?,
        )?;
    }
    let cleanup = if app_owner().is_ok() {
        step(&["action", "quit"]).map(|_| ())
    } else {
        Ok(())
    };
    let state = stored().map_err(|e| e.to_string());
    fs::write(
        root.join("final-state.json"),
        serde_json::to_vec_pretty(&state)?,
    )?;
    let requests = peer
        .observed
        .lock()
        .map_err(|_| "HTTP peer lock poisoned")?
        .clone();
    fs::write(
        root.join("provider-requests.json"),
        serde_json::to_vec_pretty(&requests)?,
    )?;
    outcome?;
    cleanup?;
    require(
        state.is_ok(),
        "final saved state could not be inspected; see final-state.json",
    )?;
    if mode == "smoke" {
        std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| peer.finish())).map_err(
            |_| "synthetic HTTP peer rejected request count/protocol; inspect runner.log",
        )?;
    }
    Ok(
        json!({"session":root,"state":state,"fixture":root.join("fixture.json"),"trace":root.join("trace.jsonl"),"application_log":root.join("app.log")}),
    )
}
fn main() -> std::process::ExitCode {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    if args.first().is_some_and(|v| v == "--help") {
        println!(
            "Development client: doctor|discover|state|launch [--global-shortcuts]|click BUTTON|action NAME|edit TEXT|assert FIELD EXPECTED|screenshot NAME. Launch with dev/verify.sh session; no host-desktop fallback."
        );
        return std::process::ExitCode::SUCCESS;
    }
    let root = match isolation::check() {
        Ok(root) => root,
        Err(error) => {
            println!(
                "{}",
                json!({"ok":false,"status":"blocked","error":error.to_string()})
            );
            return std::process::ExitCode::from(3);
        }
    };
    let is_run = args.first().is_some_and(|v| v == "run");
    let result = if is_run {
        run(&args[1..], &root)
    } else {
        record(&args)
    };
    let document = match &result {
        Ok(data) => {
            if is_run {
                json!({"ok":true,"status":"passed","data":data})
            } else {
                data.clone()
            }
        }
        Err(error) => {
            json!({"ok":false,"status":"failed","command":args.first(),"error":error.to_string(),"session":root,"diagnostics":[root.join("failure-state.json"),root.join("failure.png"),root.join("app.log"),root.join("trace.jsonl")]})
        }
    };
    if is_run {
        let saved = path_env("OFFSCREEN_ARTIFACT_DIR").and_then(|dir| {
            Ok(fs::write(
                dir.join("result.json"),
                serde_json::to_vec_pretty(&document)?,
            )?)
        });
        if let Err(error) = saved {
            println!(
                "{}",
                json!({"ok":false,"status":"failed","error":format!("could not save result.json: {error}"),"session":root})
            );
            return std::process::ExitCode::FAILURE;
        }
    }
    println!("{document}");
    if result.is_ok() {
        std::process::ExitCode::SUCCESS
    } else {
        std::process::ExitCode::FAILURE
    }
}
