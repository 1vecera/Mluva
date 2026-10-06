//! Actual disposable terminal processes and a synthetic compositor CLI.
use super::{helpers_reaped, trace};
use mluva_core::delivery::{DeliveryOptions, TargetResult, deliver_text};
use mluva_core::terminal_target::{
    capture_hyprland_terminal_target, hyprland_terminal_tracking_available,
};
use serde_json::{Value, json};
use std::{
    collections::HashMap,
    ffi::OsString,
    fs,
    io::Write,
    path::Path,
    time::{Duration, Instant},
};

pub(super) fn hold(root: &Path) -> ! {
    use std::os::unix::process::CommandExt;
    let parent = unsafe { libc::getppid() };
    assert_eq!(
        unsafe { libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGKILL) },
        0
    );
    if unsafe { libc::getppid() } != parent {
        std::process::exit(0);
    }
    loop {
        if let Ok(name) = fs::read_to_string(root.join(format!("switch-{}", std::process::id()))) {
            fs::remove_file(root.join(format!("switch-{}", std::process::id()))).unwrap();
            let error = std::process::Command::new(root.join("bin").join(name))
                .arg("--hold-terminal")
                .exec();
            panic!("fixture executable transition failed: {error}");
        }
        std::thread::sleep(Duration::from_millis(2));
    }
}

pub(super) fn hyprctl(root: &Path) -> ! {
    let spec: Value =
        serde_json::from_slice(&fs::read(root.join("fixture.json")).unwrap()).unwrap();
    let index = fs::read_to_string(root.join("hyprctl-counter"))
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .unwrap_or(0);
    fs::write(root.join("hyprctl-counter"), (index + 1).to_string()).unwrap();
    let responses = spec["terminal_responses"].as_array().unwrap();
    let response = &responses[index.min(responses.len() - 1)];
    trace(
        root,
        json!({"event":"hyprctl","args":std::env::args().skip(1).collect::<Vec<_>>(),
        "environment":std::env::vars().collect::<std::collections::BTreeMap<_,_>>(),"pid":std::process::id()}),
    );
    std::thread::sleep(Duration::from_millis(
        response["sleep_ms"].as_u64().unwrap_or(0),
    ));
    if let Some(signal) = response["signal"].as_i64() {
        unsafe { libc::raise(signal as libc::c_int) };
        panic!("fixture compositor signal did not stop the process");
    }
    if response["stderr_first"] == true {
        std::io::stderr().write_all(&vec![b'x'; 131_072]).unwrap();
    }
    let output = match response.get("raw") {
        Some(raw) => raw.as_str().unwrap().as_bytes().to_vec(),
        None => serde_json::to_vec(&response["json"]).unwrap(),
    };
    if response["fragmented"] == true {
        for byte in output {
            std::io::stdout().write_all(&[byte]).unwrap();
        }
    } else {
        std::io::stdout().write_all(&output).unwrap();
    }
    std::io::stdout().flush().unwrap();
    std::process::exit(response["exit"].as_i64().unwrap_or(0) as i32)
}

fn substitute_pids(value: &Value, pids: &[u32]) -> Value {
    match value {
        Value::String(value) if value.starts_with("$PID") => {
            json!(pids[value[4..].parse::<usize>().unwrap()])
        }
        Value::String(value) => json!(
            pids.iter()
                .enumerate()
                .fold(value.clone(), |text, (index, pid)| text
                    .replace(&format!("$PID{index}"), &pid.to_string()))
        ),
        Value::Array(values) => json!(
            values
                .iter()
                .map(|value| substitute_pids(value, pids))
                .collect::<Vec<_>>()
        ),
        Value::Object(values) => Value::Object(
            values
                .iter()
                .map(|(key, value)| (key.clone(), substitute_pids(value, pids)))
                .collect(),
        ),
        _ => value.clone(),
    }
}

pub(super) fn driver(root: &Path, spec: Value) {
    let mut processes = spec["terminal_processes"]
        .as_array()
        .unwrap()
        .iter()
        .map(|name| {
            std::process::Command::new(root.join("bin").join(name.as_str().unwrap()))
                .arg("--hold-terminal")
                .stdin(std::process::Stdio::null())
                .stdout(std::process::Stdio::null())
                .stderr(std::process::Stdio::null())
                .spawn()
                .unwrap()
        })
        .collect::<Vec<_>>();
    let pids = processes
        .iter()
        .map(std::process::Child::id)
        .collect::<Vec<_>>();
    let spec = substitute_pids(&spec, &pids);
    fs::write(
        root.join("fixture.json"),
        serde_json::to_vec(&spec).unwrap(),
    )
    .unwrap();
    let available = match spec.get("availability_environment") {
        Some(map) => {
            let environment: HashMap<String, String> = serde_json::from_value(map.clone()).unwrap();
            let environment = environment
                .into_iter()
                .map(|(key, value)| (OsString::from(key), OsString::from(value)))
                .collect();
            hyprland_terminal_tracking_available(Some(&environment))
        }
        None => hyprland_terminal_tracking_available(None),
    };
    let mut target = None;
    let mut results = vec![];
    for call in spec["calls"].as_array().unwrap() {
        let started = Instant::now();
        let result = match call["action"].as_str().unwrap() {
            "capture" => {
                target = capture_hyprland_terminal_target();
                target.as_ref().map(|target| json!({"address":target.address(),"process_id":target.process_id(),"application_identifier":target.application_identifier()})).unwrap_or(Value::Null)
            }
            "restore" => json!(target.as_ref().unwrap().restore()),
            "insert" => json!(target.as_ref().unwrap().insert_text("synthetic output")),
            "confirm" => json!(
                target
                    .as_ref()
                    .unwrap()
                    .confirm_insertion("synthetic output")
            ),
            "clone_restore" => json!(target.as_ref().unwrap().clone().restore()),
            "deliver" => {
                let target = target.as_ref().unwrap();
                let output = "synthetic output 😊\n";
                let mut direct =
                    |text: &str| -> TargetResult<Option<bool>> { Ok(target.insert_text(text)) };
                let mut confirm =
                    || -> TargetResult<Option<bool>> { Ok(target.confirm_insertion(output)) };
                let mut authorize = || -> TargetResult<bool> { Ok(target.restore()) };
                let receipt = deliver_text(
                    output,
                    true,
                    DeliveryOptions {
                        insert_directly: Some(&mut direct),
                        confirm_paste: Some(&mut confirm),
                        authorize_keyboard_paste: Some(&mut authorize),
                        application_identifier: Some(target.application_identifier()),
                        ..DeliveryOptions::default()
                    },
                )
                .unwrap();
                json!({"copied":receipt.copied,"pasted":receipt.pasted,"guidance":receipt.guidance,
                    "paste_dispatched":receipt.paste_dispatched,"paste_confirmed":receipt.paste_confirmed,"history_outcome":receipt.history_outcome()})
            }
            "environment" => {
                for (key, value) in call["values"].as_object().unwrap() {
                    // This dedicated fixture process has no threads reading its environment.
                    unsafe { std::env::set_var(key, value.as_str().unwrap()) };
                }
                Value::Null
            }
            "delete_executable" => {
                fs::remove_file(root.join("bin").join(call["name"].as_str().unwrap())).unwrap();
                Value::Null
            }
            "stop_process" => {
                let index = call["index"].as_u64().unwrap() as usize;
                processes[index].kill().unwrap();
                processes[index].wait().unwrap();
                Value::Null
            }
            "switch_executable" => {
                let index = call["index"].as_u64().unwrap() as usize;
                let name = call["name"].as_str().unwrap();
                fs::write(root.join(format!("switch-{}", pids[index])), name).unwrap();
                let deadline = Instant::now() + Duration::from_secs(2);
                loop {
                    match fs::read_link(
                        Path::new("/proc").join(pids[index].to_string()).join("exe"),
                    ) {
                        Ok(executable) if executable == root.join("bin").join(name) => break,
                        Ok(_) => {}
                        // /proc can briefly lose the executable link during exec.
                        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
                        Err(error) => panic!("actual fixture executable transition: {error}"),
                    }
                    assert!(
                        processes[index].try_wait().unwrap().is_none(),
                        "fixture terminal exited before its executable transition"
                    );
                    assert!(
                        Instant::now() < deadline,
                        "actual fixture executable transition"
                    );
                    std::thread::sleep(Duration::from_millis(2));
                }
                Value::Null
            }
            _ => panic!("unknown terminal fixture action"),
        };
        let mut observation = json!({"value":result,"query_children_reaped":helpers_reaped(root)});
        if call["deadline"] == true {
            observation["deadline_observed"] = json!(
                (Duration::from_millis(450)..Duration::from_millis(950))
                    .contains(&started.elapsed())
            );
        }
        results.push(observation);
    }
    for process in &mut processes {
        let _ = process.kill();
        process.wait().unwrap();
    }
    let rows = fs::read_to_string(root.join("trace.jsonl")).unwrap_or_default();
    let events: Vec<Value> = rows
        .lines()
        .map(|line| {
            let mut event: Value = serde_json::from_str(line).unwrap();
            event.as_object_mut().unwrap().remove("pid");
            event.as_object_mut().unwrap().remove("at_ns");
            event
        })
        .collect();
    let observed = json!({"available":available,"results":results,"events":events,
        "query_children_reaped":helpers_reaped(root),"terminal_processes_reaped":pids.iter().all(|pid| !Path::new("/proc").join(pid.to_string()).exists())});
    // Normalize only task-local locations and externally assigned process IDs.
    fn normalize(value: &mut Value, root: &Path, pids: &[u32]) {
        match value {
            Value::String(value) => *value = value.replace(root.to_str().unwrap(), "$ROOT"),
            Value::Object(values) => {
                if let Some(pid) = values.get_mut("process_id")
                    && let Some(index) = pids
                        .iter()
                        .position(|value| pid.as_u64() == Some(*value as u64))
                {
                    *pid = json!(format!("$PID{index}"));
                }
                for value in values.values_mut() {
                    normalize(value, root, pids);
                }
            }
            Value::Array(values) => {
                for value in values {
                    normalize(value, root, pids);
                }
            }
            _ => {}
        }
    }
    let mut observed = observed;
    normalize(&mut observed, root, &pids);
    println!("{observed}");
}
