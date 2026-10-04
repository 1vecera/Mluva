//! Released title queue transactions using the real native factory and isolated children.
use mluva_core::{
    config::AppConfig,
    history::{HistoryEntry, HistoryInput, HistoryStore},
    prompt_catalog::DEFAULTS,
};
use mluva_gtk::{async_runtime::DesktopRuntime, title_jobs::ConversationTitleJobs};
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::BTreeMap,
    fs,
    os::unix::fs::PermissionsExt,
    path::{Path, PathBuf},
    rc::Rc,
    thread,
    time::{Duration, Instant},
};

fn drain() {
    while glib::MainContext::default().pending() {
        glib::MainContext::default().iteration(false);
    }
}
fn until(mut predicate: impl FnMut() -> bool) {
    let end = Instant::now() + Duration::from_secs(8);
    while !predicate() {
        assert!(Instant::now() < end, "Title owner did not settle");
        drain();
        thread::sleep(Duration::from_millis(2));
    }
    drain();
}
fn settle() {
    let end = Instant::now() + Duration::from_millis(40);
    while Instant::now() < end {
        drain();
        thread::sleep(Duration::from_millis(2));
    }
}
fn records(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap_or_default()
        .lines()
        .filter_map(|line| serde_json::from_str(line).ok())
        .collect()
}
fn turns(evidence: &Path) -> Vec<Value> {
    records(&evidence.join("requests.jsonl")).iter().filter(|record|record["message"]["method"]=="turn/start").map(|record| {
        let params=&record["message"]["params"];json!({"prompt":params["input"][0]["text"],"effort":params["effort"],"service_tier":params["serviceTier"]})
    }).collect()
}
fn models(evidence: &Path) -> Vec<Value> {
    records(&evidence.join("requests.jsonl"))
        .iter()
        .filter(|record| record["message"]["method"] == "thread/start")
        .map(|record| record["message"]["params"]["model"].clone())
        .collect()
}
fn snapshot(
    jobs: &ConversationTitleJobs,
    history: &HistoryStore,
    notes: &[HistoryEntry],
    changed: &RefCell<Vec<String>>,
) -> Value {
    let titles = notes
        .iter()
        .map(|note| match history.find(&note.identifier) {
            Ok(entry) => {
                let revision: i64 = rusqlite::Connection::open(&history.database.path)
                    .unwrap()
                    .query_row(
                        "SELECT title_revision FROM transcription_history WHERE identifier=?",
                        [&entry.identifier],
                        |row| row.get(0),
                    )
                    .unwrap();
                json!({"title":entry.title,"revision":revision,"raw":entry.raw_text})
            }
            Err(_) => json!({"deleted":true}),
        })
        .collect::<Vec<_>>();
    json!({"active":jobs.active(),"pending":jobs.pending(),"titles":titles,"changed":*changed.borrow()})
}
fn isolated() -> PathBuf {
    let root = PathBuf::from(
        std::env::var_os("OFFSCREEN_SESSION_ROOT").expect("private verification runner"),
    );
    for key in ["HOME", "XDG_CONFIG_HOME", "XDG_DATA_HOME", "XAUTHORITY"] {
        assert!(PathBuf::from(std::env::var_os(key).unwrap()).starts_with(&root));
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
    let tools = root.join("title-codex-tools");
    assert_eq!(
        std::env::split_paths(&std::env::var_os("PATH").unwrap()).next(),
        Some(tools.clone())
    );
    fs::create_dir(&tools).unwrap();
    fs::set_permissions(&tools, fs::Permissions::from_mode(0o700)).unwrap();
    let peer = PathBuf::from(std::env::var_os("CARGO_TARGET_DIR").unwrap())
        .join("debug/codex-fixture-peer")
        .canonicalize()
        .unwrap();
    let quote = |path: &Path| format!("'{}'", path.to_str().unwrap().replace('\'', "'\\''"));
    let adapter = tools.join("codex");
    fs::write(
        &adapter,
        format!(
            "#!/bin/sh\nexec {} serve {} \"$@\"\n",
            quote(&peer),
            quote(&root.join("title-codex-fixture.json"))
        ),
    )
    .unwrap();
    fs::set_permissions(adapter, fs::Permissions::from_mode(0o700)).unwrap();
    root
}
fn child_cleanup(evidence: &Path) -> Vec<Value> {
    let processes = records(&evidence.join("process.jsonl"));
    until(|| {
        processes
            .iter()
            .all(|process| !Path::new(&format!("/proc/{}", process["pid"])).exists())
    });
    for process in &processes {
        assert!(!Path::new(process["cwd"].as_str().unwrap()).exists());
    }
    processes
}

#[test]
#[ignore = "requires the private display/session/network/device runner and native Codex protocol peer"]
fn released_title_jobs_and_owner_exit() {
    let root = isolated();
    let runtime = DesktopRuntime::new().unwrap();
    let fixture: Value =
        serde_json::from_str(include_str!("fixtures/released-title-jobs.json")).unwrap();
    assert_eq!(
        fixture["reference"],
        "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
    );
    let mut observations = 0;
    for row in fixture["cases"].as_array().unwrap() {
        let name = row["name"].as_str().unwrap();
        let params = &row["params"];
        let directory = tempfile::tempdir_in(&root).unwrap();
        let evidence = directory.path().join("evidence");
        fs::create_dir(&evidence).unwrap();
        let history = HistoryStore::new(directory.path().join("history.sqlite3"));
        history.initialize().unwrap();
        let config = Rc::new(RefCell::new(
            serde_json::from_value::<AppConfig>(
                params.get("initial").cloned().unwrap_or_else(|| json!({})),
            )
            .unwrap(),
        ));
        let instructions = Rc::new(RefCell::new(
            DEFAULTS
                .prompts
                .iter()
                .find(|prompt| prompt.identifier == "title")
                .unwrap()
                .default
                .clone(),
        ));
        let changed = Rc::new(RefCell::new(Vec::<String>::new()));
        let identifiers = Rc::new(RefCell::new(BTreeMap::new()));
        let mut notes = vec![];
        for i in 0..params["count"].as_u64().unwrap_or(1) {
            let raw = if params["blank"] == true {
                " \t\n".into()
            } else {
                format!(
                    "Um, note {} has 12 files and 34 folders. Keep their source.",
                    i + 1
                )
            };
            let mut entry = history
                .add(HistoryInput {
                    delivery_outcome: "ready".into(),
                    ..HistoryInput::dictation(raw, "Editable text keeps 56 notes.")
                })
                .unwrap();
            if params["named"] == true {
                entry = history
                    .update_title(&entry.identifier, Some("Human title"))
                    .unwrap();
            }
            if params["cleared"] == true {
                entry = history.update_title(&entry.identifier, None).unwrap();
            }
            if params["delete_before"] == true {
                history.delete(&entry.identifier).unwrap();
            }
            identifiers
                .borrow_mut()
                .insert(entry.identifier.clone(), format!("$NOTE{}", i + 1));
            notes.push(entry);
        }
        let gate = directory.path().join("first.release");
        let mut controls=notes.iter().map(|entry|(entry.raw_text.clone(),json!({"deltas":if params["failed"]==true {json!([42])} else {json!([params["reply"].as_str().unwrap_or("“Generated title.”")])}}))).collect::<serde_json::Map<_,_>>();
        controls.get_mut(&notes[0].raw_text).unwrap()["gate"] = json!(gate);
        let specfile = root.join("title-codex-fixture.json");
        fs::write(
            &specfile,
            serde_json::to_vec(
                &json!({"scenario":"clean","evidence":evidence,"title_controls":controls}),
            )
            .unwrap(),
        )
        .unwrap();
        let current = config.clone();
        let prompt = instructions.clone();
        let changes = changed.clone();
        let names = identifiers.clone();
        let jobs = ConversationTitleJobs::new(
            history.clone(),
            directory.path().into(),
            runtime.clone(),
            Rc::new(move || current.borrow().clone()),
            Rc::new(move || Ok(prompt.borrow().clone())),
            Rc::new(move |identifier| {
                changes
                    .borrow_mut()
                    .push(names.borrow()[identifier].clone())
            }),
        );
        for entry in &notes {
            jobs.enqueue(entry);
        }
        let mut stages =
            vec![json!({"stage":"enqueued","state":snapshot(&jobs,&history,&notes,&changed)})];
        if jobs.active() {
            until(|| turns(&evidence).len() == 1);
            stages.push(
                json!({"stage":"first-turn","state":snapshot(&jobs,&history,&notes,&changed)}),
            );
            if params["rename"] == true {
                history
                    .update_title(&notes[0].identifier, Some("Manual during request"))
                    .unwrap();
            }
            if params["clear"] == true {
                history.update_title(&notes[0].identifier, None).unwrap();
            }
            if params["delete"] == true {
                history.delete(&notes[0].identifier).unwrap();
            }
            if params["delete_queued"] == true {
                history.delete(&notes[1].identifier).unwrap();
            }
            if params["rename_queued"] == true {
                history
                    .update_title(&notes[1].identifier, Some("Manual queued title"))
                    .unwrap();
            }
            if let Some(edits) = params["config_edit"].as_object() {
                let mut document = serde_json::to_value(config.borrow().clone()).unwrap();
                document.as_object_mut().unwrap().extend(edits.clone());
                *config.borrow_mut() = serde_json::from_value(document).unwrap();
            }
            if params["prompt_edit"] == true {
                *instructions.borrow_mut() =
                    "Current instructions for the next queued title.".into();
            }
            if params["cancel"] == true {
                jobs.cancel();
            }
            if params["close"] == true {
                jobs.close();
            }
            stages.push(json!({"stage":"edited-or-cancelled","state":snapshot(&jobs,&history,&notes,&changed)}));
            fs::write(&gate, "").unwrap();
            if params["new_generation"] == true {
                let fresh = history
                    .add(HistoryInput {
                        delivery_outcome: "ready".into(),
                        ..HistoryInput::dictation(
                            "New generation has 78 notes. Preserve these.",
                            "Fresh output.",
                        )
                    })
                    .unwrap();
                identifiers
                    .borrow_mut()
                    .insert(fresh.identifier.clone(), "$NOTE2".into());
                notes.push(fresh.clone());
                controls.insert(
                    fresh.raw_text.clone(),
                    json!({"deltas":["Fresh generation title"]}),
                );
                fs::write(
                    &specfile,
                    serde_json::to_vec(
                        &json!({"scenario":"clean","evidence":evidence,"title_controls":controls}),
                    )
                    .unwrap(),
                )
                .unwrap();
                jobs.enqueue(&fresh);
            }
            until(|| !jobs.active());
        }
        if params["close"] == true {
            let fresh = history
                .add(HistoryInput {
                    delivery_outcome: "ready".into(),
                    ..HistoryInput::dictation("After close must stay unnamed.", "Fresh output.")
                })
                .unwrap();
            identifiers
                .borrow_mut()
                .insert(fresh.identifier.clone(), "$NOTE2".into());
            notes.push(fresh.clone());
            jobs.enqueue(&fresh);
        }
        settle();
        stages.push(json!({"stage":"terminal","state":snapshot(&jobs,&history,&notes,&changed)}));
        jobs.close();
        let processes = child_cleanup(&evidence);
        fs::write(
            directory.path().join(format!("{name}.native.json")),
            serde_json::to_vec_pretty(&stages).unwrap(),
        )
        .unwrap();
        assert_eq!(json!(stages), row["stages"], "{name}: title state");
        assert_eq!(
            json!(turns(&evidence)),
            row["turns"],
            "{name}: exact prompts and unmodified reasoning/speed"
        );
        assert_eq!(
            json!(models(&evidence)),
            row["models"],
            "{name}: resolved default/capture models"
        );
        assert_eq!(
            processes.len(),
            row["process_count"].as_u64().unwrap() as usize,
            "{name}: owned children"
        );
        observations += stages.len();
        println!("RELEASED_TITLE {name} PASS");
    }
    // A dropped application owner also invalidates an outstanding real turn.
    let directory = tempfile::tempdir_in(&root).unwrap();
    let evidence = directory.path().join("evidence");
    fs::create_dir(&evidence).unwrap();
    let history = HistoryStore::new(directory.path().join("history.sqlite3"));
    history.initialize().unwrap();
    let entry = history
        .add(HistoryInput::dictation(
            "Owner exit keeps 90 notes.",
            "Unchanged source.",
        ))
        .unwrap();
    let gate = directory.path().join("never.release");
    fs::write(root.join("title-codex-fixture.json"),serde_json::to_vec(&json!({"scenario":"clean","evidence":evidence,"title_controls":{entry.raw_text.clone():{"gate":gate,"deltas":["Late title must not save"]}}})).unwrap()).unwrap();
    let count = Rc::new(RefCell::new(0));
    let changes = count.clone();
    let jobs = ConversationTitleJobs::new(
        history.clone(),
        directory.path().into(),
        runtime.clone(),
        Rc::new(AppConfig::default),
        Rc::new(|| Ok("Use a short title.".into())),
        Rc::new(move |_| *changes.borrow_mut() += 1),
    );
    jobs.enqueue(&entry);
    until(|| turns(&evidence).len() == 1);
    let fallback = history.find(&entry.identifier).unwrap().title;
    drop(jobs);
    child_cleanup(&evidence);
    let end = Instant::now() + Duration::from_millis(600);
    while Instant::now() < end {
        drain();
        thread::sleep(Duration::from_millis(2));
    }
    assert_eq!(history.find(&entry.identifier).unwrap().title, fallback);
    assert_eq!(*count.borrow(), 1);
    println!(
        "RELEASED_TITLE COMPLETE {} {observations}; owner exit PASS",
        fixture["cases"].as_array().unwrap().len()
    );
}
