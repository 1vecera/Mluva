//! Synthetic Omarchy boundary. This executable imports no Mluva implementation.
use serde_json::{Value, json};
use std::{
    fs,
    io::{self, Write},
    os::unix::{
        ffi::OsStringExt,
        fs::{PermissionsExt, symlink},
    },
    path::PathBuf,
    process::{Command, Stdio},
    thread,
    time::Duration,
};
fn main() {
    let root = PathBuf::from(std::env::var_os("MLUVA_SCREENSHOT_FIXTURE_ROOT").unwrap());
    let spec: Value = serde_json::from_slice(&fs::read(root.join("picker.json")).unwrap()).unwrap();
    if std::env::args().nth(1).as_deref() == Some("--child") {
        if spec["child_ignores_term"] == true {
            unsafe {
                libc::signal(libc::SIGTERM, libc::SIG_IGN);
            }
        }
        fs::write(root.join("child.ready"), b"ready").unwrap();
        loop {
            thread::sleep(Duration::from_millis(10));
        }
    }
    let directory = PathBuf::from(std::env::var_os("OMARCHY_SCREENSHOT_DIR").unwrap());
    let runtime = root.join(spec["runtime_directory"].as_str().unwrap_or("runtime"));
    assert!(runtime.starts_with(&root));
    assert_eq!(directory.parent().unwrap(), runtime);
    if spec["ignore_term"] == true {
        unsafe {
            libc::signal(libc::SIGTERM, libc::SIG_IGN);
        }
    }
    let child = if spec["child"] == true {
        let child = Command::new(std::env::current_exe().unwrap())
            .arg("--child")
            .stdin(Stdio::null())
            .stdout(Stdio::inherit())
            .stderr(Stdio::null())
            .spawn()
            .unwrap();
        while !root.join("child.ready").exists() {
            thread::sleep(Duration::from_millis(2));
        }
        Some(child)
    } else {
        None
    };
    let name = spec["name_hex"]
        .as_str()
        .map(|text| std::ffi::OsString::from_vec(unhex(text)))
        .unwrap_or_else(|| "selected.png".into());
    let mut path = directory.join(name);
    let data = unhex(spec["png"].as_str().unwrap_or(""));
    match spec["kind"].as_str().unwrap_or("file") {
        "file" => {
            fs::write(&path, data).unwrap();
        }
        "large-image" => {
            fs::write(&path, vec![b'x'; 8 * 1024 * 1024 + 1]).unwrap();
        }
        "nested" => {
            path = directory.join("nested/image.png");
            fs::create_dir(path.parent().unwrap()).unwrap();
            fs::write(&path, data).unwrap();
        }
        "outside" => {
            path = root.join("outside.png");
            fs::write(&path, data).unwrap();
        }
        "symlink" => {
            let other = root.join("outside.png");
            fs::write(&other, data).unwrap();
            symlink(other, &path).unwrap();
        }
        "directory" => {
            fs::create_dir(&path).unwrap();
        }
        "fifo" => {
            use std::os::unix::ffi::OsStrExt;
            let name = std::ffi::CString::new(path.as_os_str().as_bytes()).unwrap();
            assert_eq!(unsafe { libc::mkfifo(name.as_ptr(), 0o600) }, 0);
        }
        "missing" => {}
        "parent-components" => {
            fs::write(&path, data).unwrap();
            path = directory.join("unused/../selected.png");
        }
        "relative" => {
            fs::write(&path, data).unwrap();
            path = path
                .strip_prefix(std::env::current_dir().unwrap())
                .unwrap()
                .to_path_buf();
        }
        other => panic!("unknown fixture {other}"),
    }
    let pid = std::process::id();
    let trace = json!({"args":std::env::args().skip(1).collect::<Vec<_>>(),"directory_mode":fs::metadata(&directory).unwrap().permissions().mode() & 0o777,"group_leader":unsafe{libc::getpgrp()} as u32==pid,"session_leader":unsafe{libc::getsid(0)} as u32==pid});
    fs::write(root.join("trace.json"), serde_json::to_vec(&trace).unwrap()).unwrap();
    fs::write(
        root.join("ready.json"),
        serde_json::to_vec(
            &json!({"pid":pid,"child":child.as_ref().map(|p|p.id()),"directory":directory}),
        )
        .unwrap(),
    )
    .unwrap();
    if spec["wait"] == true {
        while !root.join("release").exists() {
            thread::sleep(Duration::from_millis(2));
        }
    }
    let mut output = std::io::stdout();
    let bytes = if let Some(hex) = spec["output_hex"].as_str() {
        unhex(hex)
    } else if spec["large_whitespace"] == true {
        let mut bytes = vec![b' '; 8192];
        if spec["trailing_text"] == true {
            bytes.push(b'x');
        }
        bytes
    } else if spec["large_output"] == true {
        vec![b'x'; 131_072]
    } else {
        use std::os::unix::ffi::OsStrExt;
        let mut bytes = path.as_os_str().as_bytes().to_vec();
        if spec["whitespace"] == true {
            bytes.splice(0..0, b"\x0b\r\n\t ".iter().copied());
            bytes.extend(b" \t\x0b\r\n");
        } else {
            bytes.push(b'\n');
        }
        bytes
    };
    if spec["stderr"] == true {
        let _ = io::stderr().write_all(&vec![b'x'; 131_072]);
    }
    let _ = output.write_all(&bytes);
    let _ = output.flush();
    std::process::exit(spec["exit"].as_i64().unwrap_or(0) as i32);
}
fn unhex(text: &str) -> Vec<u8> {
    text.as_bytes()
        .chunks_exact(2)
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
