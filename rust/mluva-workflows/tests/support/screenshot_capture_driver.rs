//! Public native capture API driver; process configuration is external fixture data.
use mluva_workflows::screenshot_capture::{ScreenshotCapture, ScreenshotCaptureError};
use serde_json::{Value, json};
use std::{fs, path::PathBuf, time::Duration};
#[tokio::main(worker_threads = 2)]
async fn main() {
    let root = PathBuf::from(std::env::var_os("MLUVA_SCREENSHOT_FIXTURE_ROOT").unwrap());
    let spec: Value = serde_json::from_slice(&fs::read(root.join("picker.json")).unwrap()).unwrap();
    // Adopt and reap synthetic grandchildren after their picker exits.
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_CHILD_SUBREAPER, 1, 0, 0, 0) },
        0
    );
    let mut capture = Some(ScreenshotCapture::new(root.join("runtime")));
    if spec["precancel"] == true {
        capture.as_ref().unwrap().cancel();
    }
    let mut task = tokio::spawn(capture.as_ref().unwrap().run());
    let mut dropped = false;
    if spec["cancel"] == true || spec["drop_waiter"] == true || spec["drop_owner"] == true {
        tokio::time::timeout(Duration::from_secs(5), async {
            while !root.join("ready.json").exists() {
                tokio::time::sleep(Duration::from_millis(2)).await;
            }
        })
        .await
        .unwrap();
        if spec["drop_waiter"] == true {
            task.abort();
            dropped = true;
        } else if spec["drop_owner"] == true {
            capture.take();
        } else {
            capture.as_ref().unwrap().cancel();
        }
    }
    let result = if dropped {
        assert!(task.await.unwrap_err().is_cancelled());
        json!({"dropped":true})
    } else {
        match (&mut task).await.unwrap() {
            Ok(None) => json!({"cancelled":true}),
            Ok(Some(data)) => {
                json!({"png":data.iter().map(|byte|format!("{byte:02x}")).collect::<String>()})
            }
            Err(ScreenshotCaptureError::Io(error)) => json!({"io":error.raw_os_error()}),
            Err(error) => json!({"error":error.to_string()}),
        }
    };
    let ready = fs::read(root.join("ready.json"))
        .ok()
        .map(|raw| serde_json::from_slice::<Value>(&raw).unwrap());
    let mut cleanup = true;
    if let Some(ready) = &ready {
        let deadline = tokio::time::Instant::now() + Duration::from_secs(4);
        loop {
            if let Some(pid) = ready["child"].as_i64() {
                unsafe { libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) };
            }
            let alive = ["pid", "child"].iter().any(|key| {
                ready[*key]
                    .as_i64()
                    .is_some_and(|pid| PathBuf::from(format!("/proc/{pid}")).exists())
            });
            if !alive {
                break;
            }
            if tokio::time::Instant::now() >= deadline {
                cleanup = false;
                break;
            }
            tokio::time::sleep(Duration::from_millis(5)).await;
        }
    }
    let trace = fs::read(root.join("trace.json"))
        .ok()
        .map(|data| serde_json::from_slice::<Value>(&data).unwrap());
    let empty =
        fs::read_dir(root.join("runtime")).is_ok_and(|mut entries| entries.next().is_none());
    println!(
        "{}",
        json!({"result":result,"trace":trace,"directory_empty":empty,"processes_gone":cleanup})
    );
    // Clean only this fixture's known child if the production fault leaked it.
    if !cleanup && let Some(ready) = ready {
        for key in ["pid", "child"] {
            if let Some(pid) = ready[key].as_i64() {
                unsafe {
                    if libc::waitpid(pid as i32, std::ptr::null_mut(), libc::WNOHANG) == 0 {
                        libc::kill(pid as i32, libc::SIGKILL);
                        libc::waitpid(pid as i32, std::ptr::null_mut(), 0);
                    }
                }
            }
        }
    }
}
