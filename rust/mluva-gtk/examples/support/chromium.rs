//! Private ChromeDriver transport for the existing real-browser DOM peer.

use gio::prelude::FileExt;
use serde_json::{Value, json};
use std::{
    fs,
    io::{Read, Write},
    net::{TcpListener, TcpStream},
    os::unix::process::CommandExt,
    path::Path,
    process::{Child, Command},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

pub struct Chromium {
    driver: Child,
    port: u16,
    session: Option<String>,
    pub pid: u32,
    pub version: Value,
    pub build_id: Value,
    pub window: String,
    page: String,
    directory: std::path::PathBuf,
}

// ChromeDriver replies carry Content-Length. Bound both headers and body; do not
// wait for a keep-alive connection to close or depend on an external HTTP tool.
fn request(port: u16, method: &str, path: &str, value: Value) -> Result<Value, String> {
    let exchange = || -> Result<Value, String> {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(Duration::from_secs(15)))
            .map_err(|e| e.to_string())?;
        stream
            .set_write_timeout(Some(Duration::from_secs(5)))
            .map_err(|e| e.to_string())?;
        let body = if value.is_null() {
            Vec::new()
        } else {
            serde_json::to_vec(&value).map_err(|e| e.to_string())?
        };
        write!(
            stream,
            "{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nConnection: close\r\nContent-Length: {}\r\n\r\n",
            body.len()
        )
        .map_err(|e| e.to_string())?;
        stream.write_all(&body).map_err(|e| e.to_string())?;
        let mut header = Vec::new();
        while !header.ends_with(b"\r\n\r\n") {
            if header.len() >= 32 * 1024 {
                return Err("oversized ChromeDriver header".into());
            }
            let mut byte = [0];
            stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
            header.push(byte[0]);
        }
        let header = String::from_utf8(header).map_err(|e| e.to_string())?;
        let length = header
            .lines()
            .find_map(|line| {
                let (name, value) = line.split_once(':')?;
                name.eq_ignore_ascii_case("content-length")
                    .then(|| value.trim().parse::<usize>().ok())
                    .flatten()
            })
            .filter(|length| *length <= 16 * 1024 * 1024)
            .ok_or("missing/oversized ChromeDriver Content-Length")?;
        let mut body = vec![0; length];
        stream.read_exact(&mut body).map_err(|e| e.to_string())?;
        let response: Value = serde_json::from_slice(&body).map_err(|e| e.to_string())?;
        if !header.starts_with("HTTP/1.1 200 ") || !response["value"]["error"].is_null() {
            return Err(format!("{method} {path}: {response}"));
        }
        Ok(response["value"].clone())
    };
    // Chromium discovers the enabled status/address over D-Bus during startup.
    // Serve those real callbacks while the driver waits for its new session.
    thread::scope(|scope| {
        let (send, receive) = mpsc::sync_channel(1);
        scope.spawn(move || {
            let _ = send.send(exchange());
        });
        let deadline = Instant::now() + Duration::from_secs(20);
        loop {
            match receive.try_recv() {
                Ok(result) => return result,
                Err(mpsc::TryRecvError::Disconnected) => {
                    return Err("ChromeDriver worker closed".into());
                }
                Err(mpsc::TryRecvError::Empty) => {}
            }
            if Instant::now() >= deadline {
                return Err("ChromeDriver request exceeded its deadline".into());
            }
            let context = glib::MainContext::default();
            while context.pending() {
                context.iteration(false);
            }
            thread::sleep(Duration::from_millis(2));
        }
    })
}

impl Chromium {
    pub fn start(directory: &Path) -> Self {
        let profile = directory.join("profile");
        fs::create_dir(&profile).unwrap();
        // The runner already owns this short, private runtime directory and its
        // network/PID namespaces. A long evidence TMPDIR breaks SingletonSocket.
        let temporary = Path::new(&std::env::var_os("XDG_RUNTIME_DIR").unwrap())
            .join(format!("chromium-tmp-{}", std::process::id()));
        fs::create_dir(&temporary).unwrap();
        let listener = TcpListener::bind(("127.0.0.1", 0)).unwrap();
        let port = listener.local_addr().unwrap().port();
        drop(listener);
        let log = fs::File::create(directory.join("chromedriver.stdout")).unwrap();
        let driver = Command::new("chromedriver")
            .args([
                format!("--port={port}"),
                "--allowed-ips=127.0.0.1".into(),
                format!(
                    "--log-path={}",
                    directory.join("chromedriver.log").display()
                ),
            ])
            .env("TMPDIR", temporary)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .process_group(0)
            .spawn()
            .unwrap();
        let mut browser = Self {
            driver,
            port,
            session: None,
            pid: 0,
            version: Value::Null,
            build_id: Value::Null,
            window: String::new(),
            page: String::new(),
            directory: directory.into(),
        };
        let deadline = Instant::now() + Duration::from_secs(20);
        while let Err(error) = request(port, "GET", "/status", Value::Null) {
            assert!(browser.driver.try_wait().unwrap().is_none());
            assert!(
                Instant::now() < deadline,
                "ChromeDriver never became ready: {error}"
            );
            thread::sleep(Duration::from_millis(20));
        }
        let session = request(
            port,
            "POST",
            "/session",
            json!({"capabilities":{"alwaysMatch":{"browserName":"chrome","goog:chromeOptions":{
                "binary":"/usr/bin/chromium","args":[format!("--user-data-dir={}",profile.display()),
                "--no-first-run","--no-default-browser-check","--disable-background-networking",
                "--disable-sync","--disable-extensions","--password-store=basic","--ozone-platform=x11"]
            }}}}),
        )
        .unwrap();
        browser.session = Some(session["sessionId"].as_str().unwrap().into());
        let capabilities = &session["capabilities"];
        assert_eq!(capabilities["browserName"], "chrome");
        assert_eq!(
            capabilities["chrome"]["userDataDir"],
            profile.to_str().unwrap()
        );
        browser.pid = u32::try_from(capabilities["goog:processID"].as_u64().unwrap()).unwrap();
        assert_eq!(
            fs::read_link(format!("/proc/{}/exe", browser.pid)).unwrap(),
            Path::new("/usr/lib/chromium/chromium")
        );
        browser.version = capabilities["browserVersion"].clone();
        browser.build_id = browser.command(
            "goog/cdp/execute",
            json!({"cmd":"Browser.getVersion","params":{}}),
        )["revision"]
            .clone();
        fs::write(
            directory.join("chromium-capabilities.json"),
            serde_json::to_vec_pretty(capabilities).unwrap(),
        )
        .unwrap();
        let page = directory.join("target.html");
        fs::write(&page, include_str!("../firefox_text_target.html")).unwrap();
        let url = gio::File::for_path(page).uri();
        browser.page = url.to_string();
        browser.command("url", json!({"url":url.as_str()}));
        assert_eq!(
            browser.script("return location.href;", json!([])),
            url.as_str()
        );
        let title = browser.script("return document.title;", json!([]));
        let output = Command::new("xdotool")
            .args(["search", "--onlyvisible", "--pid"])
            .arg(browser.pid.to_string())
            .args(["--name", title.as_str().unwrap()])
            .output()
            .unwrap();
        assert!(output.status.success(), "private Chromium window not found");
        let windows = String::from_utf8(output.stdout).unwrap();
        let windows = windows.split_whitespace().collect::<Vec<_>>();
        assert_eq!(windows.len(), 1, "ambiguous private Chromium window");
        browser.window = windows[0].into();
        browser
    }

    fn command(&self, name: &str, parameters: Value) -> Value {
        request(
            self.port,
            "POST",
            &format!("/session/{}/{name}", self.session.as_ref().unwrap()),
            parameters,
        )
        .unwrap()
    }

    pub fn script(&self, script: &str, args: Value) -> Value {
        self.command("execute/sync", json!({"script":script,"args":args}))
    }

    pub fn focus_field(&self, kind: &str) {
        let element = self.command(
            "element",
            json!({"using":"css selector","value":format!("#{kind}")}),
        );
        let element = element["element-6066-11e4-a52e-4f735466cecf"]
            .as_str()
            .unwrap();
        self.command(&format!("element/{element}/click"), json!({}));
        assert_eq!(self.script("return document.hasFocus();", json!([])), true);
        assert_eq!(
            self.script("return document.activeElement.id;", json!([])),
            kind
        );
    }

    /// Configure an explicit user setting through Chromium's actual internal UI.
    /// This is a separate scenario from cold/default accessibility, not a launch flag.
    pub fn enable_accessibility(&self) {
        let target = request(
            self.port,
            "GET",
            &format!("/session/{}/window", self.session.as_ref().unwrap()),
            Value::Null,
        )
        .unwrap();
        let settings = self.command("window/new", json!({"type":"tab"}));
        self.command("window", json!({"handle":settings["handle"]}));
        self.command("url", json!({"url":"chrome://accessibility/"}));
        let state = "return Object.fromEntries(['native','web','screenReader'].map(id => [id, document.getElementById(id).checked]));";
        let before = self.script(state, json!([]));
        assert_eq!(before["native"], false);
        assert_eq!(before["web"], false);
        for id in ["native", "web"] {
            if self.script(state, json!([]))[id] == false {
                let element = self.command(
                    "element",
                    json!({"using":"css selector","value":format!("#{id}")}),
                );
                let element = element["element-6066-11e4-a52e-4f735466cecf"]
                    .as_str()
                    .unwrap();
                self.command(&format!("element/{element}/click"), json!({}));
                let deadline = Instant::now() + Duration::from_secs(5);
                while self.script(state, json!([]))[id] != true {
                    assert!(
                        Instant::now() < deadline,
                        "Chromium {id} setting did not change"
                    );
                    thread::sleep(Duration::from_millis(20));
                }
            }
        }
        let after = self.script(state, json!([]));
        assert_eq!(after["native"], true);
        assert_eq!(after["web"], true);
        assert_eq!(after["screenReader"], false);
        fs::write(
            self.directory.join("accessibility-settings.json"),
            serde_json::to_vec_pretty(&json!({"before":before,"after":after})).unwrap(),
        )
        .unwrap();
        // Chromium scopes these flags to the settings page's lifetime. Retain
        // that real page in its own tab while returning to the original target.
        self.command("window", json!({"handle":target}));
        assert_eq!(self.script("return location.href;", json!([])), self.page);
    }
}

impl Drop for Chromium {
    fn drop(&mut self) {
        let normal = !thread::panicking();
        let quit = self.session.take().map(|session| {
            let result = request(
                self.port,
                "DELETE",
                &format!("/session/{session}"),
                Value::Null,
            );
            let exited = !Path::new(&format!("/proc/{}", self.pid)).exists();
            (result, exited)
        });
        // Always stop and reap the owned driver before reporting a failed Quit.
        // Record browser disappearance beforehand: forced cleanup is not acceptance.
        let group = -(self.driver.id() as i32);
        unsafe { libc::kill(group, libc::SIGTERM) };
        let deadline = Instant::now() + Duration::from_secs(2);
        while self.driver.try_wait().ok().flatten().is_none() {
            if Instant::now() >= deadline {
                unsafe { libc::kill(group, libc::SIGKILL) };
                let _ = self.driver.wait();
                break;
            }
            thread::sleep(Duration::from_millis(10));
        }
        unsafe { libc::kill(group, libc::SIGKILL) };
        if normal && let Some((result, exited)) = quit {
            assert!(result.is_ok(), "Chromium did not quit normally: {result:?}");
            assert!(exited, "Chromium survived its normal WebDriver Quit");
        }
    }
}
