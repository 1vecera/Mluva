//! Explicit acceptance of the requested 2.0.2 Rewrite disclosure. Historical
//! fixtures stay immutable; legacy workflows use an explicitly expanded profile.
use super::*;

const PROMPT: &str = "Write a custom instruction, then press Ctrl+Enter to send";

pub(super) fn disclosure(binary: &Path) -> bool {
    let manifest = binary
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join(".mluva-native.json");
    let version = if manifest.exists() {
        let manifest: Value = serde_json::from_slice(&fs::read(manifest).unwrap()).unwrap();
        assert_eq!(manifest["implementation"], "rust");
        manifest["version"].as_str().unwrap().to_owned()
    } else {
        env!("CARGO_PKG_VERSION").to_owned()
    };
    let version: Vec<u32> = version
        .split('.')
        .map(|part| part.parse().unwrap())
        .collect();
    assert_eq!(version.len(), 3);
    version.as_slice() >= [2, 0, 2].as_slice()
}

fn preference(root: &Path) -> PathBuf {
    root.join("data/mluva/rewrite-panel-expanded.json")
}

pub(super) fn source_application(binary: &Path, root: &Path) -> Command {
    if disclosure(binary) {
        let path = preference(root);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, b"true").unwrap();
        fs::set_permissions(path, fs::Permissions::from_mode(0o600)).unwrap();
    }
    application(binary, root)
}

pub(super) fn source_snapshot(binary: &Path, snapshot: &Value) -> Value {
    fn expand(value: &mut Value, headings: &BTreeMap<String, String>) {
        match value {
            Value::Object(fields) => {
                if let Some(frame) = fields.get_mut("frame")
                    && let Some(complete) = frame
                        .get("sha256")
                        .and_then(Value::as_str)
                        .and_then(|hash| headings.get(hash))
                {
                    frame["sha256"] = json!(complete);
                }
                if let Some(Value::Array(names)) = fields.get_mut("names")
                    && names.contains(&json!(["text box", PROMPT]))
                {
                    names.extend([
                        json!(["label", "Rewrite"]),
                        json!(["toggle button", "Rewrite"]),
                    ]);
                    names.sort_by_key(|name| {
                        (
                            name[0].as_str().unwrap().to_owned(),
                            name[1].as_str().unwrap().to_owned(),
                        )
                    });
                }
                for value in fields.values_mut() {
                    expand(value, headings);
                }
            }
            Value::Array(values) => values.iter_mut().for_each(|value| expand(value, headings)),
            _ => {}
        }
    }
    let mut expected = snapshot.clone();
    if disclosure(binary) {
        let repaint: Value = serde_json::from_str(include_str!(
            "../fixtures/released-bootstrap-complete-heading.json"
        ))
        .unwrap();
        assert_eq!(
            repaint["reference"],
            "5202477edfe4b5d8bacfa5b2e9fd6eadd9624f7f"
        );
        let headings = repaint["observations"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| {
                (
                    row["original_rgb_sha256"].as_str().unwrap().to_owned(),
                    row["complete_heading_rgb_sha256"]
                        .as_str()
                        .unwrap()
                        .to_owned(),
                )
            })
            .collect();
        expand(&mut expected, &headings);
    }
    expected
}

fn toggle(accessibility: &Accessibility) -> Option<(String, String)> {
    accessibility.button("Rewrite").filter(|node| {
        accessibility
            .call(node, "org.a11y.atspi.Accessible", "GetRoleName", None)
            .unwrap()
            .get::<(String,)>()
            .unwrap()
            .0
            == "toggle button"
            && accessibility
                .call(node, "org.a11y.atspi.Accessible", "GetState", None)
                .unwrap()
                .child_value(0)
                .get::<Vec<u32>>()
                .unwrap()[0]
                & (1 << 25)
                != 0
    })
}

pub(super) fn activate(accessibility: &Accessibility) {
    let node = toggle(accessibility).unwrap();
    assert_eq!(
        accessibility
            .call(
                &node,
                "org.a11y.atspi.Action",
                "DoAction",
                Some(&(0_i32,).to_variant())
            )
            .unwrap()
            .get::<(bool,)>(),
        Some((true,))
    );
    settle();
}

fn state(accessibility: &Accessibility, expanded: bool) -> Value {
    let node = toggle(accessibility).expect("the public Rewrite disclosure must exist");
    let bits = accessibility
        .call(&node, "org.a11y.atspi.Accessible", "GetState", None)
        .unwrap()
        .child_value(0)
        .get::<Vec<u32>>()
        .unwrap();
    assert_eq!(
        bits[0] & (1 << 20) != 0,
        expanded,
        "remembered toggle selection"
    );
    assert_ne!(bits[0] & (1 << 24), 0, "idle disclosure is available");
    let (names, items) = accessibility.visible_content();
    assert_eq!(
        names.contains(&("text box".into(), PROMPT.into())),
        expanded
    );
    for label in ["Polish", "Structure", "More"] {
        assert_eq!(
            names.iter().any(|(_, name)| name == label),
            expanded,
            "{label}"
        );
    }
    json!({"expanded":expanded,"names":names,"items":items})
}

pub(super) fn exercise(
    binary: &Path,
    base: &Path,
    bus: &Bus,
    accessibility: &Accessibility,
    config: &Value,
) {
    if !disclosure(binary) {
        return;
    }
    let root = base.join("rewrite-disclosure");
    fs::create_dir_all(root.join("config/mluva")).unwrap();
    fs::write(
        root.join("config/mluva/config.json"),
        serde_json::to_vec(config).unwrap(),
    )
    .unwrap();
    let mut observed = vec![];
    for (index, initial) in [false, true, false].into_iter().enumerate() {
        let log = fs::File::create(root.join(format!("application-{index}.log"))).unwrap();
        // No seeded preference here: this is the actual default and public
        // toggle/Quit/relaunch boundary, separate from the historical profile.
        let mut process = Process(
            application(binary, &root)
                .stdout(log.try_clone().unwrap())
                .stderr(log)
                .spawn()
                .unwrap(),
        );
        until(|| bus.owner().is_some() && visible(process.0.id()));
        until(|| toggle(accessibility).is_some());
        settle();
        observed.push(state(accessibility, initial));
        if index < 2 {
            activate(accessibility);
            until(|| {
                fs::read(preference(&root))
                    .ok()
                    .and_then(|bytes| serde_json::from_slice::<bool>(&bytes).ok())
                    == Some(!initial)
            });
            settle();
            observed.push(state(accessibility, !initial));
            assert_eq!(
                fs::metadata(preference(&root))
                    .unwrap()
                    .permissions()
                    .mode()
                    & 0o777,
                0o600
            );
        }
        bus.action("quit");
        assert_eq!(process.finish(), 0);
        until(|| bus.owner().is_none());
        assert!(!visible(process.0.id()));
        assert_eq!(
            fs::read(root.join(format!("application-{index}.log"))).unwrap(),
            b""
        );
    }
    fs::write(
        root.join("observed.json"),
        serde_json::to_vec_pretty(&observed).unwrap(),
    )
    .unwrap();
    eprintln!(
        "matched Rewrite collapsed default and both choices across three normal Quit/relaunch processes"
    );
}

/// Preserve the entire legacy 1100×800 viewport. Give the current window the
/// independently observed extra 52 pixels, then remove only the disclosure row.
/// The full unmodified capture and exact projection metadata remain on disk.
pub(super) fn frame(root: &Path, window: &str, stage: &str) -> Value {
    let accessibility = Accessibility::open();
    let disclosure = toggle(&accessibility);
    if disclosure.is_some() {
        assert!(
            accessibility
                .visible_content()
                .0
                .contains(&("text box".into(), PROMPT.into()))
        );
        assert!(
            Command::new("xdotool")
                .args(["windowsize", "--sync", window, "1100", "852"])
                .status()
                .unwrap()
                .success()
        );
        settle();
    }
    let path = root.join(format!("managed-{stage}.png"));
    assert!(
        Command::new("/usr/bin/import")
            .args(["-window", window])
            .arg(&path)
            .status()
            .unwrap()
            .success()
    );
    let image = gtk::gdk_pixbuf::Pixbuf::from_file(&path).unwrap();
    let bytes = image.read_pixel_bytes();
    let raw = json!({"width":image.width(),"height":image.height(),"channels":image.n_channels(),
        "sha256":hash(bytes.as_ref())});
    let Some(node) = disclosure else {
        return raw;
    };
    assert_eq!(
        (
            image.width(),
            image.height(),
            image.n_channels(),
            image.rowstride()
        ),
        (1100, 852, 3, 3300)
    );
    let bounds = accessibility
        .call(
            &node,
            "org.a11y.atspi.Component",
            "GetExtents",
            Some(&(1_u32,).to_variant()),
        )
        .unwrap()
        .get::<((i32, i32, i32, i32),)>()
        .unwrap()
        .0;
    assert_eq!((bounds.0, bounds.2, bounds.3), (11, 81, 36));
    // The pinned source/current X11 renderer adds 10 pixels to public WINDOW
    // coordinates. The independently measured row has 8 pixels on either side.
    let start = usize::try_from(bounds.1 + 10 - 8).unwrap();
    assert!(start > 42 && start + 52 < 852);
    let mut projected = bytes.as_ref()[..start * 3300].to_vec();
    projected.extend_from_slice(&bytes.as_ref()[(start + 52) * 3300..]);
    assert_eq!(projected.len(), 1100 * 800 * 3);
    let legacy = json!({"width":1100,"height":800,"channels":3,"sha256":hash(&projected)});
    fs::write(root.join(format!("managed-{stage}-projection.json")),
        serde_json::to_vec_pretty(&json!({"raw":raw,"disclosure_window_bounds":bounds,"removed_rows":[start,start+52],"legacy_viewport":legacy})).unwrap()).unwrap();
    assert!(
        Command::new("xdotool")
            .args(["windowsize", "--sync", window, "1100", "800"])
            .status()
            .unwrap()
            .success()
    );
    settle();
    legacy
}
