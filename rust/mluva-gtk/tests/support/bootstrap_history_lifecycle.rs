//! Retained-audio failures and root shutdown/reopen through the existing app owner.
use super::*;

fn stored(root: &Path) -> Value {
    let mut rows = normalized_history(root);
    for row in rows.as_array_mut().unwrap() {
        if !row["delivery_ms"].is_null() {
            assert_eq!(row["delivery_outcome"], "copied");
            assert!(row["delivery_ms"].as_u64().unwrap() <= 2000);
            row["delivery_ms"] = json!("$SAMPLED");
        }
    }
    json!({"history":rows,"replies":table(root,"conversation_rewrites","identifier")})
}

fn runtime(root: &Path, specification: &Value) {
    let mut specification = specification.clone();
    specification["root"] = json!(root);
    let binary = QWEN_RUNTIME.binary(&root.join("data/mluva"), "cpu");
    write(&binary.with_file_name("fixture.json"), &specification);
}

fn quit(root: &Path, bus: &Bus, process: &mut Process, stage: &str, log: &str) -> Value {
    let start = Instant::now();
    bus.action("quit");
    let exit = process.finish();
    until(|| bus.owner().is_none());
    assert!(start.elapsed() < Duration::from_secs(10));
    let actual = json!({"stage":stage,"exit_code":exit,"application_name_absent":bus.owner().is_none(),
        "resources":resources(root),"store":stored(root),"retained_audio":recovery_audio(root,&history(root),false),
        "clipboard":live::clipboard(),"app_log_bytes":fs::read(root.join(log)).unwrap().len()});
    write(&root.join(format!("history-{stage}.json")), &actual);
    actual
}

pub(super) fn exercise(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    events: &RefCell<Vec<Value>>,
    capture: &Value,
    full_pcm: &[u8],
    binaries: &Path,
) {
    let source: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-history-lifecycle.json"
    ))
    .unwrap();
    let mut fixture: Value = serde_json::from_str(include_str!(
        "../fixtures/released-bootstrap-managed-recovery.json"
    ))
    .unwrap();
    assert_eq!(
        hash(include_bytes!(
            "../fixtures/released-bootstrap-managed-recovery.json"
        )),
        source["base"]["sha256"]
    );
    for table in ["widgets", "stores", "observations"] {
        fixture[table]
            .as_array_mut()
            .unwrap()
            .extend(source[table].as_array().unwrap().iter().cloned());
    }
    let first = &full_pcm[..capture["pcm"]["first_bytes"].as_u64().unwrap() as usize];
    let fresh =
        &full_pcm[full_pcm.len() - fixture["fresh"]["last_bytes"].as_u64().unwrap() as usize..];
    assert_eq!(hash(fresh), fixture["fresh"]["sha256"]);
    let accessibility = Accessibility::open();
    for case in source["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let root = base.join(format!("managed-history-{name}"));
        let mut configuration = capture.clone();
        configuration["runtime_spec"] = fixture["cases"][0]["runtime_spec"].clone();
        for (key, value) in source["config_overrides"].as_object().unwrap() {
            configuration["config"][key] = value.clone();
        }
        setup(&root, &configuration, first, binaries, 0);
        let clip = std::env::split_paths(&std::env::var_os("PATH").unwrap())
            .map(|path| path.join("xclip"))
            .find(|path| path.is_file())
            .unwrap();
        symlink(clip, root.join("tools/xclip")).unwrap();
        events.borrow_mut().clear();
        let mut process = live::launch(binary, &root, "application-0.log");
        until(|| bus.owner().is_some() && visible(process.0.id()));
        let mut window = place_window(process.0.id());
        live::set_clipboard("untouched timeout recovery clipboard");
        let mut states = vec![];
        let mut exits = vec![];
        let mut failed = None;
        for index in case["states"].as_array().unwrap() {
            let mut expected = fixture["observations"][index.as_u64().unwrap() as usize].clone();
            expected["widgets"] =
                fixture["widgets"][expected["widgets"].as_u64().unwrap() as usize].clone();
            expected["store"] =
                fixture["stores"][expected["store"].as_u64().unwrap() as usize].clone();
            let stage = expected["stage"].as_str().unwrap();
            match stage {
                "recording" | "fresh-recording" => {
                    if stage == "fresh-recording" {
                        set_recording_audio(&root, fresh);
                    }
                    bus.action("record");
                    let end = Instant::now() + Duration::from_secs(15);
                    while !root.join("tools/raw.ready.json").exists() {
                        assert!(Instant::now() < end, "complete paced recording input");
                        settle();
                    }
                }
                "processing" => {
                    bus.action("record");
                    until(|| trace(&root).iter().any(|row| row["kind"] == "fragment"));
                }
                "expired" => {
                    until(|| status(events)[1] == "error");
                    failed = Some(history(&root)[0].clone());
                }
                "failure-history" | "reopened-history" => {
                    bus.action("history");
                    until(|| {
                        live::elements(&accessibility)
                            .iter()
                            .any(|node| node.role == "button" && node.name == "Retry transcription")
                    });
                }
                "retry-failed" => {
                    runtime(
                        &root,
                        &json!({"device":"cpu","keep_alive":true,"responses":[{"status":500}]}),
                    );
                    click_retry(&accessibility);
                    until(|| {
                        resources(&root)["processes"] == 4
                            && resources(&root)["alive"] == json!(vec![false; 4])
                    });
                    assert_eq!(history(&root)[0], failed.as_ref().unwrap().clone());
                }
                "retry-held" => {
                    runtime(
                        &root,
                        &json!({"device":"cpu","keep_alive":true,"responses":[{"hold":true}]}),
                    );
                    click_retry(&accessibility);
                    until(|| {
                        resources(&root)["processes"] == 4
                            && trace(&root)
                                .iter()
                                .filter(|row| row["kind"] == "request")
                                .count()
                                == 4
                    });
                }
                "repeated-retry" => click_retry(&accessibility),
                "record-blocked" => bus.action("record"),
                "reopened" => {
                    let actual = quit(&root, bus, &mut process, "quit-held", "application-0.log");
                    let mut expected = case["exits"][0].clone();
                    assert_eq!(
                        case["native_improvements"],
                        json!([{"exit":"quit-held","path":["resources","alive",3],"source":true,"native":false}])
                    );
                    assert_eq!(
                        expected["resources"]["alive"],
                        json!([false, false, false, true])
                    );
                    expected["resources"]["alive"][3] = json!(false);
                    assert_eq!(
                        actual, expected,
                        "only the released held-worker leak improves after root Quit"
                    );
                    exits.push(actual);
                    assert_eq!(history(&root)[0], failed.as_ref().unwrap().clone());
                    runtime(&root, &fixture["fresh"]["runtime_spec"]);
                    process = live::launch(binary, &root, "application-1.log");
                    until(|| bus.owner().is_some() && visible(process.0.id()));
                    window = place_window(process.0.id());
                }
                "recovered-after-failure" | "recovered-after-reopen" => {
                    runtime(&root, &fixture["fresh"]["runtime_spec"]);
                    click_retry(&accessibility);
                    until(|| {
                        history(&root)[0]["raw_text"] == "hello"
                            && resources(&root)["alive"] == json!(vec![false; 5])
                    });
                }
                "fresh-terminal" => {
                    bus.action("record");
                    until(|| {
                        history(&root).len() == 2
                            && resources(&root)["alive"] == json!(vec![false; 6])
                    });
                    until(|| live::clipboard() == "hello");
                    let end = Instant::now() + Duration::from_secs(10);
                    while status(events)[1] != "hidden" {
                        assert!(Instant::now() < end, "actual completion widget timeout");
                        settle();
                    }
                }
                other => panic!("unknown released History action {other}"),
            }
            let mut actual = recovery_state(&root, bus, events, &accessibility, &window, stage);
            actual["store"] = stored(&root);
            write(&root.join(format!("history-{stage}.json")), &actual);
            assert_eq!(actual, expected, "assembled History {name}: {stage}");
            if let Some(failed) = &failed {
                let entry = history(&root)[0].clone();
                for field in ["identifier", "created_at", "retained_audio_path"] {
                    assert_eq!(entry[field], failed[field], "same recovery audio owner");
                }
            }
            states.push(actual);
        }
        let log = if name == "quit-reopen" {
            "application-1.log"
        } else {
            "application-0.log"
        };
        let actual = quit(&root, bus, &mut process, "quit-final", log);
        assert_eq!(
            actual,
            case["exits"].as_array().unwrap().last().unwrap().clone()
        );
        exits.push(actual);
        let (mut rows, health_statuses) = protocol(&root);
        rows.retain(|row| row["kind"] != "fragment");
        let actual = json!({"trace":rows,"health_statuses":health_statuses,"exit_code":0,"post_exit_resources":resources(&root),"app_log_bytes":0});
        assert_eq!(
            actual, source["protocol"],
            "full independent History retry transport contract"
        );
        write(
            &root.join("history-observed.json"),
            &json!({"states":states,"exits":exits,"protocol":actual}),
        );
        eprintln!(
            "matched assembled History {name}: {} states, real failure/guards, retained ownership and explicit recovery",
            states.len()
        );
    }
}
