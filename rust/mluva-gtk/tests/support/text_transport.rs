use gio::prelude::*;
use mluva_gtk::text_target::{
    DeliveryTargetSnapshot, FocusedTextTargetTracker, TextTargetSnapshot,
};
use serde_json::{Value, json};
use std::{
    fs,
    path::{Path, PathBuf},
    process::{Child, Command},
    thread,
    time::{Duration, Instant},
};

pub fn settle(duration: Duration) {
    let deadline = Instant::now() + duration;
    let context = glib::MainContext::default();
    while Instant::now() < deadline {
        while context.pending() {
            context.iteration(false);
        }
        thread::sleep(Duration::from_millis(5));
    }
}

pub fn write(path: &Path, value: &Value) {
    let temporary = path.with_extension("temporary");
    fs::write(&temporary, serde_json::to_vec(value).unwrap()).unwrap();
    fs::rename(temporary, path).unwrap();
}

pub struct Peer {
    process: Child,
    pub directory: PathBuf,
    pub executable: PathBuf,
    serial: u64,
}
impl Peer {
    pub fn new(directory: PathBuf, example: &str) -> Self {
        fs::create_dir(&directory).unwrap();
        let executable = std::env::current_exe()
            .unwrap()
            .parent()
            .unwrap()
            .parent()
            .unwrap()
            .join("examples")
            .join(example)
            .canonicalize()
            .expect("build the requested native target example first");
        let log = fs::File::create(directory.join("target.log")).unwrap();
        let process = Command::new(&executable)
            .arg(&directory)
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let mut peer = Self {
            process,
            directory,
            executable,
            serial: 0,
        };
        let deadline = Instant::now() + Duration::from_secs(25);
        while !peer.directory.join("observed.json").exists() {
            assert!(peer.process.try_wait().unwrap().is_none(), "target exited");
            assert!(
                Instant::now() < deadline,
                "target did not expose its observer"
            );
            settle(Duration::from_millis(25));
        }
        peer
    }
    pub fn observed(&self) -> Value {
        serde_json::from_slice(&fs::read(self.directory.join("observed.json")).unwrap()).unwrap()
    }
    pub fn request(&mut self, mut value: Value) {
        self.serial += 1;
        value["serial"] = self.serial.into();
        write(&self.directory.join("request.json"), &value);
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if self.observed()["serial"] == self.serial {
                settle(Duration::from_millis(180));
                return;
            }
            assert!(
                Instant::now() < deadline,
                "target request not acknowledged: {value}"
            );
            assert!(
                self.process.try_wait().unwrap().is_none(),
                "target exited during request"
            );
            settle(Duration::from_millis(20));
        }
    }
    pub fn stop(&mut self) {
        if self.process.try_wait().unwrap().is_none() {
            self.process.kill().unwrap();
            self.process.wait().unwrap();
        }
    }
    pub fn await_successful_exit(&mut self) {
        let deadline = Instant::now() + Duration::from_secs(5);
        loop {
            if let Some(status) = self.process.try_wait().unwrap() {
                assert!(status.success(), "private peer exited unsuccessfully");
                return;
            }
            assert!(Instant::now() < deadline, "private peer did not close");
            settle(Duration::from_millis(20));
        }
    }
}
impl Drop for Peer {
    fn drop(&mut self) {
        self.stop();
    }
}

pub struct Monitor {
    process: Child,
    path: PathBuf,
}
impl Monitor {
    pub fn new(path: PathBuf) -> Self {
        let log = fs::File::create(&path).unwrap();
        let mut process = Command::new("dbus-monitor")
            .args([
                "--address",
                &std::env::var("AT_SPI_BUS_ADDRESS").unwrap(),
                "type='method_call',interface='org.a11y.atspi.Text',member='GetText'",
            ])
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap();
        let deadline = Instant::now() + Duration::from_secs(3);
        loop {
            let log = fs::read_to_string(&path).unwrap();
            if log.contains("member=NameLost") {
                break;
            }
            assert!(
                process.try_wait().unwrap().is_none(),
                "private monitor failed: {log}"
            );
            assert!(
                Instant::now() < deadline,
                "monitor never acquired monitoring privileges: {log}"
            );
            settle(Duration::from_millis(10));
        }
        Self { process, path }
    }
    pub fn finish(mut self) -> Value {
        // SIGINT flushes dbus-monitor's private file before the exit is reaped.
        assert_eq!(
            unsafe { libc::kill(self.process.id() as i32, libc::SIGINT) },
            0
        );
        self.process.wait().unwrap();
        let log = fs::read_to_string(&self.path).unwrap();
        let mut offsets = Vec::new();
        for message in log.split("method call ").skip(1) {
            if message.lines().next().unwrap().contains("member=GetText") {
                let numbers = message
                    .lines()
                    .filter_map(|line| line.trim().strip_prefix("int32 "))
                    .map(|value| value.parse::<i32>().unwrap())
                    .collect::<Vec<_>>();
                assert_eq!(
                    numbers.len(),
                    2,
                    "not a bounded explicit GetText range: {message}"
                );
                offsets.push(numbers);
            }
        }
        json!(offsets)
    }
}
impl Drop for Monitor {
    fn drop(&mut self) {
        if self.process.try_wait().unwrap().is_none() {
            self.process.kill().unwrap();
            self.process.wait().unwrap();
        }
    }
}

pub fn snapshot(target: Option<&TextTargetSnapshot>, executable: &Path) -> Value {
    target.map_or(Value::Null, |target| json!({
        "selected_text":target.selected_text(), "selection":target.selection(), "caret":target.caret_offset(),
        "editable":target.editable_text_available(), "identity_matches":target.application_identifier() == executable.to_str(),
    }))
}
pub fn delivery(tracker: &FocusedTextTargetTracker) -> Option<TextTargetSnapshot> {
    tracker
        .capture_delivery_target()
        .map(|target| match target {
            DeliveryTargetSnapshot::Text(target) => target,
            DeliveryTargetSnapshot::Terminal(_) => {
                panic!("private X11 session must not capture a host compositor")
            }
        })
}

pub fn bus_name_has_owner(name: &str) -> bool {
    let session = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE).unwrap();
    session
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "NameHasOwner",
            Some(&(name,).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            1_000,
            gio::Cancellable::NONE,
        )
        .unwrap()
        .get::<(bool,)>()
        .unwrap()
        .0
}
