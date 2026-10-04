//! Drive the released native editor through AT-SPI and private X11 input, using
//! the Rust narration executable and real PCM/HTTP boundaries.
use glib::variant::ToVariant;
use serde_json::{Value, json};
use std::{
    fs,
    io::Write,
    os::unix::{fs::symlink, process::CommandExt},
    path::{Path, PathBuf},
    process::{Child, Command, Stdio},
    thread,
    time::{Duration, Instant},
};
#[path = "support/accessibility.rs"]
#[allow(dead_code)]
mod accessibility;
#[path = "../../mluva-workflows/tests/support/http.rs"]
mod http;
use accessibility::Accessibility;

fn command(program: &str, args: &[&str]) -> Vec<u8> {
    let output = Command::new(program).args(args).output().unwrap();
    assert!(
        output.status.success(),
        "{program}: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    output.stdout
}
fn text(program: &str, args: &[&str]) -> String {
    String::from_utf8(command(program, args))
        .unwrap()
        .trim()
        .to_owned()
}
fn checksum(bytes: &[u8]) -> String {
    let mut process = Command::new("sha256sum")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .spawn()
        .unwrap();
    process.stdin.take().unwrap().write_all(bytes).unwrap();
    let result = process.wait_with_output().unwrap();
    assert!(result.status.success());
    String::from_utf8(result.stdout)
        .unwrap()
        .split_whitespace()
        .next()
        .unwrap()
        .into()
}
fn pixels(path: &Path) -> String {
    checksum(&command("magick", &[path.to_str().unwrap(), "rgba:-"]))
}
fn key(name: &str) {
    command("xdotool", &["key", "--clearmodifiers", name]);
}
fn drag(top: &str, bottom: &str) {
    command(
        "xdotool",
        &[
            "mousemove",
            "240",
            top,
            "mousedown",
            "1",
            "mousemove",
            "--sync",
            "1000",
            bottom,
            "mouseup",
            "1",
        ],
    );
}
fn memory_gone() -> bool {
    !fs::read_dir("/dev/shm").unwrap().any(|entry| {
        entry
            .unwrap()
            .file_name()
            .to_string_lossy()
            .starts_with("mluva-audio-")
    })
}
fn data_files(root: &Path) -> Vec<String> {
    fn walk(path: &Path, root: &Path, result: &mut Vec<String>) {
        if path.is_dir() {
            for entry in fs::read_dir(path).unwrap() {
                walk(&entry.unwrap().path(), root, result);
            }
        } else if path.is_file() {
            result.push(
                path.strip_prefix(root)
                    .unwrap()
                    .to_string_lossy()
                    .into_owned(),
            );
        }
    }
    let mut result = vec![];
    walk(root, root, &mut result);
    result.sort();
    result
}

struct Editor(Child);
impl Editor {
    #[track_caller]
    fn until(&mut self, mut predicate: impl FnMut() -> bool) {
        let end = Instant::now() + Duration::from_secs(8);
        while !predicate() {
            assert!(self.0.try_wait().unwrap().is_none(), "editor exited");
            assert!(Instant::now() < end, "editor did not settle");
            thread::sleep(Duration::from_millis(20));
        }
    }
    fn save(&mut self, path: &Path) {
        let previous = path.metadata().unwrap().modified().unwrap();
        key("ctrl+s");
        self.until(|| path.metadata().unwrap().modified().unwrap() != previous);
    }
}
impl Drop for Editor {
    fn drop(&mut self) {
        if self.0.try_wait().unwrap().is_none() {
            unsafe {
                libc::kill(-(self.0.id() as i32), libc::SIGTERM);
            }
            let deadline = Instant::now() + Duration::from_secs(3);
            while self.0.try_wait().unwrap().is_none() && Instant::now() < deadline {
                thread::sleep(Duration::from_millis(10));
            }
            if self.0.try_wait().unwrap().is_none() {
                unsafe {
                    libc::kill(-(self.0.id() as i32), libc::SIGKILL);
                }
            }
            let _ = self.0.wait();
        }
    }
}

impl Accessibility {
    fn click(&self, editor: &mut Editor, label: &str) {
        let mut button = None;
        editor.until(|| {
            button = self.button(label);
            button.is_some()
        });
        assert_eq!(
            self.call(
                &button.unwrap(),
                "org.a11y.atspi.Action",
                "DoAction",
                Some(&(0i32,).to_variant())
            )
            .unwrap()
            .get::<(bool,)>(),
            Some((true,))
        );
    }
}

#[test]
#[ignore = "requires private X11/AT-SPI/input plus the pinned Tensaku artifact and native CLI binaries"]
fn native_narration_uses_real_editor_canvas_undo_and_png_export() {
    let private =
        PathBuf::from(std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private runner required"));
    for name in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(name).unwrap()).starts_with(&private));
    }
    assert_ne!(
        fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!Path::new(device).exists());
    }
    assert_eq!(std::env::var("GDK_BACKEND").unwrap(), "x11");
    assert!(std::env::var_os("WAYLAND_DISPLAY").is_none());
    assert!(memory_gone());
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-editor-narration.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let executable =
        PathBuf::from(std::env::var_os("MLUVA_TEST_EDITOR").expect("pinned editor required"));
    assert_eq!(
        checksum(&fs::read(&executable).unwrap()),
        fixture["editor_sha256"]
    );
    let binaries = PathBuf::from(
        std::env::var_os("MLUVA_TEST_NATIVE_BIN_DIR").expect("native binary directory required"),
    );
    let root = private.join("native-editor-narration");
    fs::create_dir(&root).unwrap();
    fs::create_dir(root.join("tools")).unwrap();
    fs::create_dir_all(root.join("config/mluva")).unwrap();
    symlink(
        binaries.join("meeting-audio-fixture-peer"),
        root.join("tools/pw-record"),
    )
    .unwrap();
    fs::write(
        root.join("tools/test-config.json"),
        serde_json::to_vec(&fixture["pcm"]).unwrap(),
    )
    .unwrap();
    let mut server = http::Peer::new(&[
        json!({"route":"speech","status":200,"payload":{"text":"Narration selected area 71","language_code":"eng","language_probability":0.98,"transcription_id":"synthetic-id"}}),
    ]);
    let mut config = fixture["config"].clone();
    config["transcription_base_url"] = json!(format!("{}/v1", server.address));
    fs::write(
        root.join("config/mluva/config.json"),
        serde_json::to_vec(&config).unwrap(),
    )
    .unwrap();
    let source = root.join("source.png");
    let output = root.join("output.png");
    command(
        "magick",
        &["-size", "900x550", "xc:#f7f7f7", source.to_str().unwrap()],
    );
    let mut clip = Command::new("xclip")
        .args(["-selection", "clipboard", "-in"])
        .stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .unwrap();
    clip.stdin
        .take()
        .unwrap()
        .write_all(b"untouched editor clipboard")
        .unwrap();
    assert!(clip.wait().unwrap().success());
    let log = fs::File::create(root.join("editor.log")).unwrap();
    let mut editor = Editor(
        Command::new(executable)
            .args([
                "--filename",
                source.to_str().unwrap(),
                "--output-filename",
                output.to_str().unwrap(),
                "--annotation-size-factor",
                "1",
                "--narration-command",
                binaries.join("mluva-narrate").to_str().unwrap(),
                "--disable-notifications",
            ])
            .env(
                "PATH",
                format!(
                    "{}:{}",
                    root.join("tools").display(),
                    std::env::var("PATH").unwrap()
                ),
            )
            .env("XDG_CONFIG_HOME", root.join("config"))
            .env("XDG_DATA_HOME", root.join("data"))
            .env("XDG_RUNTIME_DIR", root.join("runtime"))
            .env("TZ", "UTC")
            .env("GSETTINGS_BACKEND", "memory")
            .process_group(0)
            .stdin(Stdio::null())
            .stdout(log.try_clone().unwrap())
            .stderr(log)
            .spawn()
            .unwrap(),
    );
    let accessibility = Accessibility::open();
    editor.until(|| accessibility.button("Add narration").is_some());
    let window = text(
        "xdotool",
        &[
            "search",
            "--onlyvisible",
            "--pid",
            &editor.0.id().to_string(),
        ],
    )
    .lines()
    .last()
    .unwrap()
    .to_owned();
    command("xdotool", &["windowactivate", "--sync", &window]);
    accessibility.click(&mut editor, "Add narration");
    editor.until(|| accessibility.button("Choose text area").is_some());
    drag("360", "490");
    let ready = root.join("tools/raw.ready.json");
    editor.until(|| ready.exists());
    accessibility.click(&mut editor, "Stop narration");
    editor.until(|| output.exists());
    editor.until(|| accessibility.button("Add narration").is_some());
    let completed_pixels = pixels(&output);
    let source_pixels = pixels(&source);
    let words = text(
        "tesseract",
        &[output.to_str().unwrap(), "stdout", "--psm", "11"],
    );
    let first_png = fs::read(&output).unwrap();
    fs::write(root.join("completed.png"), &first_png).unwrap();
    fs::remove_file(&ready).unwrap();
    accessibility.click(&mut editor, "Add narration");
    editor.until(|| accessibility.button("Choose text area").is_some());
    drag("540", "650");
    editor.until(|| ready.exists());
    key("Escape");
    editor.until(|| accessibility.button("Add narration").is_some());
    editor.until(memory_gone);
    assert_eq!(fs::read(&output).unwrap(), first_png);
    editor.save(&output);
    assert_eq!(pixels(&output), completed_pixels);
    key("ctrl+z");
    editor.save(&output);
    let undo_pixels = pixels(&output);
    let undo_words = text(
        "tesseract",
        &[output.to_str().unwrap(), "stdout", "--psm", "11"],
    );
    key("ctrl+y");
    editor.save(&output);
    let redo_pixels = pixels(&output);
    let actual = json!({"ocr":words,"completed_pixels":completed_pixels,"source_pixels":source_pixels,"undo_pixels":undo_pixels,"undo_ocr":undo_words,"redo_pixels":redo_pixels,"clipboard":text("xclip", &["-selection","clipboard","-out"]),"requests":server.finish(),"memory_gone":memory_gone(),"data_files":data_files(&root.join("data"))});
    fs::write(
        root.join("observed.json"),
        serde_json::to_vec_pretty(&actual).unwrap(),
    )
    .unwrap();
    assert_eq!(actual, fixture["result"]);
    let log = fs::read_to_string(root.join("editor.log")).unwrap();
    assert!(
        !["WARNING", "CRITICAL", "panicked", "ERROR"]
            .iter()
            .any(|word| log.contains(word)),
        "{log}"
    );
    eprintln!(
        "matched real editor text, exact rendered pixels, cancel, undo/redo, clipboard and HTTP; evidence {}",
        root.display()
    );
}
