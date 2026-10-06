//! Disposable Firefox/Chromium target and independent DOM observer; never install this peer.

#[path = "support/chromium.rs"]
mod chromium;

use gio::prelude::*;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};

fn private_root() -> PathBuf {
    let root = PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").unwrap())
        .canonicalize()
        .unwrap();
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(
            PathBuf::from(std::env::var_os(key).unwrap())
                .canonicalize()
                .unwrap()
                .starts_with(&root),
            "{key} must belong to the disposable session"
        );
    }
    assert_eq!(std::env::var("XDG_SESSION_TYPE").unwrap(), "x11");
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(std::env::var_os("HYPRLAND_INSTANCE_SIGNATURE").is_none());
    assert!(
        std::env::var("AT_SPI_BUS_ADDRESS")
            .unwrap()
            .starts_with("unix:abstract=offscreen-atspi-")
    );
    assert_ne!(
        fs::read_link("/proc/self/ns/net").unwrap().as_os_str(),
        std::env::var_os("MLUVA_HOST_NET_NS").unwrap(),
        "the browser must run in a separate network namespace"
    );
    for path in ["/dev/uinput", "/dev/input", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(path).exists(), "device access is forbidden");
    }
    assert!(!Path::new("/run/dbus/system_bus_socket").exists());
    assert_eq!(
        PathBuf::from(std::env::var_os("XDG_RUNTIME_DIR").unwrap())
            .canonicalize()
            .unwrap(),
        root.join("runtime").canonicalize().unwrap()
    );
    root
}

fn write(path: &Path, value: &Value) {
    let temporary = path.with_extension("temporary");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}

struct Firefox {
    process: Child,
    stream: Option<TcpStream>,
    sequence: u32,
    capabilities: Value,
    window: String,
}
impl Firefox {
    fn start(directory: &Path) -> Self {
        let profile = directory.join("profile");
        fs::create_dir(&profile).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let preferences = json!({
            "marionette.port":port, "accessibility.force_disabled":0, "browser.backup.enabled":false,
            "browser.shell.checkDefaultBrowser":false, "browser.startup.homepage":"about:blank",
            "browser.startup.page":0, "browser.newtabpage.enabled":false,
            "browser.newtabpage.activity-stream.feeds.telemetry":false,
            "datareporting.healthreport.uploadEnabled":false, "toolkit.telemetry.enabled":false,
            "network.captive-portal-service.enabled":false, "network.connectivity-service.enabled":false,
            "network.trr.mode":5, "signon.rememberSignons":false,
            "widget.use-xdg-desktop-portal.settings":0, "widget.use-xdg-desktop-portal.file-picker":0,
            "widget.use-xdg-desktop-portal.mime-handler":0, "widget.use-xdg-desktop-portal.open-uri":0,
            "widget.use-xdg-desktop-portal.native-messaging":0, "widget.use-xdg-desktop-portal.location":0,
        });
        let preferences = preferences
            .as_object()
            .unwrap()
            .iter()
            .map(|(key, value)| {
                format!(
                    "user_pref({}, {value});",
                    serde_json::to_string(key).unwrap()
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        fs::write(profile.join("user.js"), preferences).unwrap();
        let log = fs::File::create(directory.join("firefox.log")).unwrap();
        let process = Command::new("firefox")
            .args(["--no-remote", "--new-instance", "--profile"])
            .arg(&profile)
            .args(["--marionette", "about:blank"])
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap();
        // Own the browser before readiness checks, so every failure closes its process group.
        let mut browser = Self {
            process,
            stream: None,
            sequence: 0,
            capabilities: Value::Null,
            window: String::new(),
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            if let Ok(stream) = TcpStream::connect(("127.0.0.1", port)) {
                stream
                    .set_read_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                stream
                    .set_write_timeout(Some(Duration::from_secs(5)))
                    .unwrap();
                browser.stream = Some(stream);
                break;
            }
            assert!(browser.process.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "Firefox never exposed Marionette"
            );
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(20));
        }
        let handshake = browser.read();
        assert_eq!(handshake["applicationType"], "gecko");
        assert_eq!(handshake["marionetteProtocol"], 3);
        let session = browser.command(
            "WebDriver:NewSession",
            json!({"capabilities":{"alwaysMatch":{}}}),
        );
        browser.capabilities = session["capabilities"].clone();
        assert_eq!(browser.capabilities["browserName"], "firefox");
        assert_eq!(browser.capabilities["moz:headless"], false);
        assert_eq!(browser.capabilities["moz:processID"], browser.process.id());
        assert_eq!(
            browser.capabilities["moz:profile"].as_str().unwrap(),
            profile.to_str().unwrap()
        );
        let page = directory.join("target.html");
        fs::write(&page, include_str!("firefox_text_target.html")).unwrap();
        let url = gio::File::for_path(page).uri();
        browser.command("WebDriver:Navigate", json!({"url":url.as_str()}));
        assert_eq!(
            browser.script("return location.href;", json!([])),
            url.as_str()
        );
        let title = browser.script("return document.title;", json!([]));
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--pid"])
            .arg(browser.process.id().to_string())
            .args(["--name", title.as_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success(), "private Firefox window not found");
        let windows = String::from_utf8(output.stdout).unwrap();
        let windows = windows.split_whitespace().collect::<Vec<_>>();
        assert_eq!(windows.len(), 1, "ambiguous private Firefox window");
        browser.window = windows[0].into();
        browser.focus();
        browser
    }
    fn read(&mut self) -> Value {
        let stream = self.stream.as_mut().unwrap();
        let mut prefix = String::new();
        loop {
            let mut byte = [0];
            stream.read_exact(&mut byte).unwrap();
            if byte[0] == b':' {
                break;
            }
            assert!(byte[0].is_ascii_digit() && prefix.len() < 10);
            prefix.push(char::from(byte[0]));
        }
        let length = prefix.parse::<usize>().unwrap();
        assert!(length <= 16 * 1024 * 1024);
        let mut bytes = vec![0; length];
        stream.read_exact(&mut bytes).unwrap();
        serde_json::from_slice(&bytes).unwrap()
    }
    fn command(&mut self, name: &str, parameters: Value) -> Value {
        self.sequence += 1;
        let bytes = serde_json::to_vec(&json!([0, self.sequence, name, parameters])).unwrap();
        let stream = self.stream.as_mut().unwrap();
        write!(stream, "{}:", bytes.len()).unwrap();
        stream.write_all(&bytes).unwrap();
        let response = self.read();
        assert_eq!(response[0], 1);
        assert_eq!(response[1], self.sequence);
        assert!(response[2].is_null(), "{name}: {response}");
        response[3].clone()
    }
    fn script(&mut self, script: &str, args: Value) -> Value {
        self.command(
            "WebDriver:ExecuteScript",
            json!({"script":script,"args":args}),
        )["value"]
            .clone()
    }
    fn focus(&self) {
        assert!(
            Command::new("xdotool")
                .args(["windowactivate", "--sync", &self.window])
                .stdin(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
}
impl Drop for Firefox {
    fn drop(&mut self) {
        drop(self.stream.take());
        let group = -(self.process.id() as i32);
        unsafe { libc::kill(group, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(2);
        loop {
            if self.process.try_wait().ok().flatten().is_some() {
                break;
            }
            if Instant::now() >= deadline {
                unsafe { libc::kill(group, libc::SIGKILL) };
                let _ = self.process.wait();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        // The private PID namespace also contains/reaps every descendant on exit.
        unsafe { libc::kill(group, libc::SIGKILL) };
    }
}

enum Browser {
    Firefox(Firefox),
    Chromium(chromium::Chromium),
}
impl Browser {
    fn enable_accessibility(&self) {
        match self {
            Self::Chromium(browser) => browser.enable_accessibility(),
            Self::Firefox(_) => panic!("Chromium accessibility setting requires Chromium"),
        }
    }
    fn setup(&mut self, request: Value) {
        if let Self::Chromium(browser) = self {
            browser.script(
                "return window.setup(arguments[0]);",
                json!([request.clone()]),
            );
            browser.focus_field(request["kind"].as_str().unwrap());
        }
        self.script("return window.setup(arguments[0]);", json!([request]));
    }
    fn script(&mut self, script: &str, args: Value) -> Value {
        match self {
            Self::Firefox(browser) => browser.script(script, args),
            Self::Chromium(browser) => browser.script(script, args),
        }
    }
    fn window(&self) -> &str {
        match self {
            Self::Firefox(browser) => &browser.window,
            Self::Chromium(browser) => &browser.window,
        }
    }
    fn focus(&self) {
        assert!(
            Command::new("xdotool")
                .args(["windowactivate", "--sync", self.window()])
                .stdin(Stdio::null())
                .status()
                .unwrap()
                .success()
        );
    }
    fn metadata(&self) -> Value {
        let (pid, version, build_id) = match self {
            Self::Firefox(browser) => (
                browser.process.id(),
                &browser.capabilities["browserVersion"],
                &browser.capabilities["moz:buildID"],
            ),
            Self::Chromium(browser) => (browser.pid, &browser.version, &browser.build_id),
        };
        let identity = fs::read_link(format!("/proc/{pid}/exe")).unwrap();
        json!({"version":version,"build_id":build_id,"identity":identity.to_str().unwrap(),"pid":pid,"window":self.window()})
    }
}

fn main() {
    let root = private_root();
    let directory = PathBuf::from(std::env::args_os().nth(1).unwrap())
        .canonicalize()
        .unwrap();
    assert!(directory.starts_with(root));
    let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    let info = gio::DBusNodeInfo::for_xml("<node><interface name='org.a11y.Status'><property name='IsEnabled' type='b' access='read'/><property name='ScreenReaderEnabled' type='b' access='read'/></interface><interface name='org.a11y.Bus'><method name='GetAddress'><arg type='s' direction='out'/></method></interface></node>").unwrap();
    let registration = session
        .register_object("/org/a11y/bus", &info.interfaces()[0])
        .property(|_, _, _, _, property| (property == "IsEnabled").to_variant())
        .build()
        .unwrap();
    let address = session
        .register_object("/org/a11y/bus", &info.interfaces()[1])
        .method_call(|_, _, _, _, method, _, invocation| {
            assert_eq!(method, "GetAddress");
            invocation.return_value(Some(
                &(std::env::var("AT_SPI_BUS_ADDRESS").unwrap(),).to_variant(),
            ));
        })
        .build()
        .unwrap();
    let ownership = session
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&("org.a11y.Bus", 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1_000,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .get::<(u32,)>()
        .unwrap();
    assert_eq!(ownership.0, 1);
    if std::env::var_os("MLUVA_BROWSER_WAIT_FOR_START").is_some() {
        assert_eq!(std::env::var("MLUVA_BROWSER_ENGINE").unwrap(), "chromium");
        write(
            &directory.join("status-ready.json"),
            &json!({"pid":std::process::id()}),
        );
        let deadline = Instant::now() + Duration::from_secs(20);
        while !directory.join("start-browser").exists() {
            assert!(
                Instant::now() < deadline,
                "client did not start the browser"
            );
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(10));
        }
    }
    let mut browser = match std::env::var("MLUVA_BROWSER_ENGINE").as_deref() {
        Ok("chromium") => Browser::Chromium(chromium::Chromium::start(&directory)),
        Ok("firefox") | Err(std::env::VarError::NotPresent) => {
            Browser::Firefox(Firefox::start(&directory))
        }
        engine => panic!("unsupported private browser: {engine:?}"),
    };
    let metadata = browser.metadata();
    let request_path = directory.join("request.json");
    let observed_path = directory.join("observed.json");
    let loop_ = glib::MainLoop::new(None, false);
    let control = loop_.clone();
    let mut serial = 0;
    let source = glib::timeout_add_local(Duration::from_millis(20), move || {
        if let Ok(bytes) = fs::read(&request_path) {
            let request: Value = serde_json::from_slice(&bytes).unwrap();
            if let Some(next) = request["serial"].as_u64().filter(|next| *next != serial) {
                match request["operation"].as_str().unwrap() {
                    "setup" => {
                        browser.focus();
                        browser.setup(request);
                    }
                    "focus" | "caret" | "selection" | "replace" => {
                        if request["operation"] == "focus" {
                            browser.focus();
                        }
                        browser.script("return window.change(arguments[0]);", json!([request]));
                    }
                    "enable_accessibility" => browser.enable_accessibility(),
                    "quit" => control.quit(),
                    _ => panic!("unknown browser target request"),
                }
                serial = next;
            }
        }
        let observed = browser.script("return window.observe();", json!([]));
        write(
            &observed_path,
            &json!({"serial":serial,"metadata":metadata,"observed":observed}),
        );
        glib::ControlFlow::Continue
    });
    loop_.run();
    source.remove();
    session.unregister_object(registration).unwrap();
    session.unregister_object(address).unwrap();
}
