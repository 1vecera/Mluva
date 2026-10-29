//! Hyprland's side of annotation: two temporary bindings that exist only while a recording is
//! annotated, and read-only IPC for pointer, outputs, the active window and focus events.
use mluva_core::annotations::AnnotationModifier;
use serde_json::Value;
use std::{
    future::Future,
    path::PathBuf,
    process::Stdio,
    time::{Duration, Instant},
};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt};

pub const INK: &str = "annotate-ink";
pub const PIN: &str = "annotate-pin";
const APP: &str = "com.mluva.Linux";
const DESCRIPTION: &str = "Mluva annotation: ";
const TIMEOUT: Duration = Duration::from_secs(3);

/// A session's bindings are kept under its own token, so a late cleanup of one recording can
/// never remove the next one's.
pub fn valid_token(token: &str) -> bool {
    (16..=64).contains(&token.len()) && token.bytes().all(|b| b.is_ascii_hexdigit())
}

/// The key reports press and release only with `ignore_mods`; `non_consuming` preserves normal
/// typing. `transparent` prevents another key release from hiding its release. The modified click
/// is consumed so it cannot select text in the app. Leftover owned bindings are removed first.
pub fn add_script(token: &str, modifier: AnnotationModifier) -> String {
    assert!(valid_token(token));
    let (key, name, _) = binding(modifier);
    format!(
        "{REMOVE_ALL}\
_G.mluva_annotation = {{ [\"{token}\"] = {{\n\
  hl.bind(\"{key}\", hl.dsp.global(\"{APP}:{INK}\"), {{ non_consuming = true, ignore_mods = true, transparent = true, description = \"{DESCRIPTION}{INK}\" }}),\n\
  hl.bind(\"{name} + mouse:272\", hl.dsp.global(\"{APP}:{PIN}\"), {{ description = \"{DESCRIPTION}{PIN}\" }}),\n\
}} }}\n"
    )
}

/// Removes only this session's handles; other applications' Shift bindings stay.
pub fn remove_script(token: &str) -> String {
    assert!(valid_token(token));
    format!(
        "local owned = (_G.mluva_annotation or {{}})[\"{token}\"]\n\
if owned then for _, binding in ipairs(owned) do binding:remove() end _G.mluva_annotation[\"{token}\"] = nil end\n"
    )
}

pub fn available() -> bool {
    signature().is_some()
}

fn signature() -> Option<String> {
    std::env::var("HYPRLAND_INSTANCE_SIGNATURE")
        .ok()
        .filter(|value| !value.is_empty() && !value.contains(['/', '\0']))
}

fn socket(name: &str) -> Option<PathBuf> {
    let runtime = std::env::var_os("XDG_RUNTIME_DIR")?;
    Some(
        PathBuf::from(runtime)
            .join("hypr")
            .join(signature()?)
            .join(name),
    )
}

async fn hyprctl(args: &[&str]) -> Result<String, String> {
    let mut command = tokio::process::Command::new("hyprctl");
    command
        .args(args)
        .stdin(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    let output = tokio::time::timeout(TIMEOUT, command.output())
        .await
        .map_err(|_| "Hyprland did not answer in time.".to_owned())?
        .map_err(|_| "Could not contact Hyprland.".to_owned())?;
    if !output.status.success() || output.stdout.len() > 1_048_576 {
        return Err("Hyprland rejected the annotation bindings.".into());
    }
    String::from_utf8(output.stdout).map_err(|_| "Hyprland returned invalid data.".into())
}

fn ours(binding: &Value, id: &str) -> bool {
    binding["description"].as_str() == Some(&format!("{DESCRIPTION}{id}"))
}

/// Map each supported modifier to its Hyprland key name, chord name and modifier mask.
fn binding(modifier: AnnotationModifier) -> (&'static str, &'static str, u64) {
    match modifier {
        AnnotationModifier::Shift => ("Shift_L", "SHIFT", 1),
        AnnotationModifier::Control => ("Control_L", "CTRL", 4),
        AnnotationModifier::Alt => ("Alt_L", "ALT", 8),
        AnnotationModifier::Super => ("Super_L", "SUPER", 64),
    }
}

/// The modifier on its own and its left-click chord must both be free.
fn conflicting(binds: &[Value], modifier: AnnotationModifier) -> bool {
    let (modifier_key, _, modifier_mask) = binding(modifier);
    binds.iter().any(|bound| {
        let key = bound["key"].as_str().unwrap_or("");
        let mask = bound["modmask"].as_u64();
        let held = key.eq_ignore_ascii_case(modifier_key) && mask == Some(0);
        let click = key.eq_ignore_ascii_case("mouse:272") && mask == Some(modifier_mask);
        (held || click) && !ours(bound, INK) && !ours(bound, PIN)
    })
}

/// Runs one `hyprctl` command line and returns its output.
pub trait Hyprctl {
    fn run(&self, args: Vec<String>) -> impl Future<Output = Result<String, String>>;
}
/// The real `hyprctl`.
pub struct System;
impl Hyprctl for System {
    async fn run(&self, args: Vec<String>) -> Result<String, String> {
        let args: Vec<&str> = args.iter().map(String::as_str).collect();
        hyprctl(&args).await
    }
}

// A leading Lua comment looks like an option to hyprctl's argument parser.
fn eval(script: &str) -> Vec<String> {
    vec!["eval".into(), format!("do\n{script}\nend")]
}

async fn binds(hyprctl: &impl Hyprctl) -> Result<Vec<Value>, String> {
    serde_json::from_str(&hyprctl.run(vec!["-j".into(), "binds".into()]).await?)
        .map_err(|_| "Hyprland returned invalid binding data.".into())
}

/// Add both bindings and confirm Hyprland holds them. Once the bindings have been sent, every
/// failure removes them again, including a timeout after Hyprland already applied them.
pub async fn bind(token: &str, modifier: AnnotationModifier) -> Result<(), String> {
    if !available() {
        return Err("Screen annotation needs Hyprland.".into());
    }
    bind_with(&System, token, modifier).await
}

pub async fn bind_with(
    hyprctl: &impl Hyprctl,
    token: &str,
    modifier: AnnotationModifier,
) -> Result<(), String> {
    if conflicting(&binds(hyprctl).await?, modifier) {
        return Err("The selected key or its click shortcut already has a Hyprland binding, so screen annotation stays off for this recording.".into());
    }
    let applied = async {
        let response = hyprctl.run(eval(&add_script(token, modifier))).await?;
        if response.trim() != "ok" {
            return Err("Hyprland could not add the annotation bindings.".to_owned());
        }
        let actual = binds(hyprctl).await?;
        if ![INK, PIN]
            .iter()
            .all(|id| actual.iter().any(|bound| ours(bound, id)))
        {
            return Err("Hyprland did not confirm the annotation bindings.".to_owned());
        }
        Ok(())
    }
    .await;
    if applied.is_err() {
        unbind_with(hyprctl, token).await;
    }
    applied
}

/// Remove both bindings; repeated removal is harmless.
pub async fn unbind(token: &str) -> bool {
    unbind_with(&System, token).await
}

async fn unbind_with(hyprctl: &impl Hyprctl, token: &str) -> bool {
    hyprctl
        .run(eval(&remove_script(token)))
        .await
        .is_ok_and(|response| response.trim() == "ok")
}

/// Remove every recording's bindings, for leftovers of a Mluva that was killed with its watchdog.
pub const REMOVE_ALL: &str = "for _, owned in pairs(_G.mluva_annotation or {}) do for _, binding in ipairs(owned) do binding:remove() end end\n_G.mluva_annotation = {}\n";

pub async fn unbind_all() -> bool {
    available()
        && System
            .run(eval(REMOVE_ALL))
            .await
            .is_ok_and(|response| response.trim() == "ok")
}

/// One request on Hyprland's command socket, without spawning `hyprctl`.
async fn request(command: &str) -> Result<Value, String> {
    let path = socket(".socket.sock").ok_or("Hyprland is not running.")?;
    let exchange = async {
        let mut stream = tokio::net::UnixStream::connect(&path).await.ok()?;
        stream.write_all(command.as_bytes()).await.ok()?;
        let mut reply = Vec::new();
        (&mut stream)
            .take(1_048_576)
            .read_to_end(&mut reply)
            .await
            .ok()?;
        serde_json::from_slice(&reply).ok()
    };
    tokio::time::timeout(TIMEOUT, exchange)
        .await
        .ok()
        .flatten()
        .ok_or_else(|| "Hyprland did not answer.".into())
}

#[derive(Clone, Debug, PartialEq)]
pub struct Output {
    pub name: String,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
    pub scale: f64,
    pub focused: bool,
}
impl Output {
    pub fn contains(&self, x: f64, y: f64) -> bool {
        x >= self.x && y >= self.y && x < self.x + self.width && y < self.y + self.height
    }
}

/// Outputs in layout (logical) coordinates.
pub fn outputs_from(value: &Value) -> Vec<Output> {
    value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|monitor| {
            let scale = monitor["scale"].as_f64().filter(|s| *s > 0.0)?;
            let (mut width, mut height) = (
                monitor["width"].as_f64()? / scale,
                monitor["height"].as_f64()? / scale,
            );
            if monitor["transform"].as_i64().is_some_and(|t| t % 2 == 1) {
                std::mem::swap(&mut width, &mut height);
            }
            Some(Output {
                name: monitor["name"].as_str()?.into(),
                x: monitor["x"].as_f64()?,
                y: monitor["y"].as_f64()?,
                width: width.round(),
                height: height.round(),
                scale,
                focused: monitor["focused"].as_bool().unwrap_or(false),
            })
        })
        .collect()
}

pub async fn outputs() -> Vec<Output> {
    request("j/monitors")
        .await
        .map(|value| outputs_from(&value))
        .unwrap_or_default()
}

pub async fn cursor() -> Option<(f64, f64)> {
    let value = request("j/cursorpos").await.ok()?;
    Some((value["x"].as_f64()?, value["y"].as_f64()?))
}

/// A visible window's box in layout coordinates, and whether it has focus.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Visible {
    pub rect: [f64; 4],
    pub active: bool,
}

fn rect(client: &Value) -> Option<[f64; 4]> {
    let at = client["at"].as_array()?;
    let size = client["size"].as_array()?;
    let (x, y) = (at.first()?.as_f64()?, at.get(1)?.as_f64()?);
    let (w, h) = (size.first()?.as_f64()?, size.get(1)?.as_f64()?);
    (w > 0.0 && h > 0.0).then_some([x, y, x + w, y + h])
}

/// Windows shown on `output`: mapped, not hidden, on its active or open special workspace.
/// Only geometry and focus are read; titles and classes are not.
pub fn visible_windows(monitors: &Value, clients: &Value, output: &str) -> Vec<Visible> {
    let Some(monitor) = monitors
        .as_array()
        .into_iter()
        .flatten()
        .find(|monitor| monitor["name"].as_str() == Some(output))
    else {
        return vec![];
    };
    let shown: Vec<i64> = [
        &monitor["activeWorkspace"]["id"],
        &monitor["specialWorkspace"]["id"],
    ]
    .into_iter()
    .filter_map(Value::as_i64)
    .filter(|id| *id != 0)
    .collect();
    clients
        .as_array()
        .into_iter()
        .flatten()
        .filter(|client| {
            client["mapped"].as_bool() != Some(false)
                && client["hidden"].as_bool() != Some(true)
                && client["workspace"]["id"]
                    .as_i64()
                    .is_some_and(|id| shown.contains(&id))
        })
        .filter_map(|client| {
            Some(Visible {
                rect: rect(client)?,
                active: client["focusHistoryID"].as_i64() == Some(0),
            })
        })
        .collect()
}

pub async fn windows(output: &str) -> Vec<Visible> {
    match (request("j/monitors").await, request("j/clients").await) {
        (Ok(monitors), Ok(clients)) => visible_windows(&monitors, &clients, output),
        _ => vec![],
    }
}

/// Focus changes, by window class only; titles are never read.
#[derive(Clone, Debug, PartialEq)]
pub enum FocusEvent {
    Window { class: String },
    Workspace { name: String },
}

/// Parse one `EVENT>>DATA` line from Hyprland's event socket.
pub fn focus_event(line: &str) -> Option<FocusEvent> {
    let (event, data) = line.split_once(">>")?;
    match event {
        "activewindow" => {
            let class = data.split_once(',').map_or(data, |(class, _)| class);
            Some(FocusEvent::Window {
                class: class.chars().take(120).collect(),
            })
        }
        "workspacev2" => {
            let name = data.split_once(',').map_or(data, |(_, name)| name);
            Some(FocusEvent::Workspace {
                name: name.chars().take(60).collect(),
            })
        }
        _ => None,
    }
}

/// Stream focus changes until the receiver is dropped or the socket closes.
pub async fn watch(sender: tokio::sync::mpsc::UnboundedSender<(Instant, FocusEvent)>) {
    let Some(path) = socket(".socket2.sock") else {
        return;
    };
    let Ok(stream) = tokio::net::UnixStream::connect(&path).await else {
        return;
    };
    let mut lines = tokio::io::BufReader::new(stream).lines();
    loop {
        tokio::select! {
            _ = sender.closed() => return,
            line = lines.next_line() => match line {
                Ok(Some(line)) => {
                    if let Some(event) = focus_event(&line)
                        && sender.send((Instant::now(), event)).is_err()
                    {
                        return;
                    }
                }
                _ => return,
            },
        }
    }
}

/// Capture one output to a private JPEG: several times faster than PNG at panel resolutions,
/// and re-encoded losslessly once cropped and inked.
pub async fn grab(output: &str, path: &std::path::Path) -> bool {
    let mut command = tokio::process::Command::new("grim");
    command
        .args(["-t", "jpeg", "-q", "90", "-o", output])
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .kill_on_drop(true);
    matches!(
        tokio::time::timeout(TIMEOUT, command.status()).await,
        Ok(Ok(status)) if status.success()
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::{cell::RefCell, collections::VecDeque};

    /// A scripted `hyprctl`: answers in order and records every command line.
    struct Fake {
        answers: RefCell<VecDeque<Result<String, String>>>,
        calls: RefCell<Vec<Vec<String>>>,
    }
    impl Fake {
        fn new(answers: &[Result<&str, &str>]) -> Self {
            Self {
                answers: RefCell::new(
                    answers
                        .iter()
                        .map(|a| a.map(str::to_owned).map_err(str::to_owned))
                        .collect(),
                ),
                calls: RefCell::new(vec![]),
            }
        }
        fn unbound(&self, token: &str) -> bool {
            self.calls.borrow().iter().any(|call| {
                call.first().is_some_and(|c| c == "eval") && call[1].contains(&remove_script(token))
            })
        }
    }
    impl Hyprctl for Fake {
        async fn run(&self, args: Vec<String>) -> Result<String, String> {
            self.calls.borrow_mut().push(args);
            self.answers
                .borrow_mut()
                .pop_front()
                .unwrap_or_else(|| Ok("ok".into()))
        }
    }

    #[tokio::test]
    async fn bindings_sent_are_removed_on_every_later_failure() {
        let ours = r#"[{"key":"Shift_L","modmask":0,"description":"Mluva annotation: annotate-ink"},{"key":"mouse:272","modmask":1,"description":"Mluva annotation: annotate-pin"}]"#;
        // The eval times out after Hyprland may already have applied it.
        let timeout = Fake::new(&[Ok("[]"), Err("Hyprland did not answer in time.")]);
        assert!(
            bind_with(&timeout, TOKEN, AnnotationModifier::Shift)
                .await
                .is_err()
        );
        assert!(timeout.unbound(TOKEN));
        // The verifying query fails.
        let unverified = Fake::new(&[Ok("[]"), Ok("ok"), Err("Could not contact Hyprland.")]);
        assert!(
            bind_with(&unverified, TOKEN, AnnotationModifier::Shift)
                .await
                .is_err()
        );
        assert!(unverified.unbound(TOKEN));
        // Hyprland answers but holds only one binding.
        let partial = Fake::new(&[
            Ok("[]"),
            Ok("ok"),
            Ok(r#"[{"key":"Shift_L","modmask":0,"description":"Mluva annotation: annotate-ink"}]"#),
        ]);
        assert!(
            bind_with(&partial, TOKEN, AnnotationModifier::Shift)
                .await
                .is_err()
        );
        assert!(partial.unbound(TOKEN));
        // A conflict is refused before anything is sent.
        let conflict = Fake::new(&[Ok(
            r#"[{"key":"mouse:272","modmask":1,"description":"Select"}]"#,
        )]);
        assert!(
            bind_with(&conflict, TOKEN, AnnotationModifier::Shift)
                .await
                .is_err()
        );
        assert_eq!(conflict.calls.borrow().len(), 1);
        // Success leaves the bindings in place.
        let bound = Fake::new(&[Ok("[]"), Ok("ok"), Ok(ours)]);
        assert!(
            bind_with(&bound, TOKEN, AnnotationModifier::Shift)
                .await
                .is_ok()
        );
        assert!(!bound.unbound(TOKEN));
    }

    #[test]
    fn visible_windows_are_those_on_the_outputs_shown_workspaces() {
        let monitors = json!([
            {"name":"DP-1","activeWorkspace":{"id":2},"specialWorkspace":{"id":-98}},
            {"name":"HDMI-A-1","activeWorkspace":{"id":5},"specialWorkspace":{"id":0}}
        ]);
        let clients = json!([
            {"at":[0,30],"size":[960,1050],"workspace":{"id":2},"mapped":true,"hidden":false,"focusHistoryID":1,"title":"private"},
            {"at":[300,300],"size":[400,300],"workspace":{"id":-98},"mapped":true,"hidden":false,"focusHistoryID":0},
            {"at":[960,30],"size":[960,1050],"workspace":{"id":3},"mapped":true,"hidden":false,"focusHistoryID":2},
            {"at":[0,0],"size":[100,100],"workspace":{"id":2},"mapped":true,"hidden":true,"focusHistoryID":3},
            {"at":[1920,0],"size":[800,600],"workspace":{"id":5},"mapped":true,"hidden":false,"focusHistoryID":4}
        ]);
        let shown = visible_windows(&monitors, &clients, "DP-1");
        assert_eq!(
            shown,
            [
                Visible {
                    rect: [0.0, 30.0, 960.0, 1080.0],
                    active: false
                },
                Visible {
                    rect: [300.0, 300.0, 700.0, 600.0],
                    active: true
                },
            ]
        );
        assert_eq!(visible_windows(&monitors, &clients, "HDMI-A-1").len(), 1);
        assert!(visible_windows(&monitors, &clients, "missing").is_empty());
    }

    const TOKEN: &str = "0123456789abcdef0123456789abcdef";

    #[test]
    fn bindings_add_and_remove_only_owned_handles() {
        let script = add_script(TOKEN, AnnotationModifier::Shift);
        assert!(script.contains("hl.bind(\"Shift_L\", hl.dsp.global(\"com.mluva.Linux:annotate-ink\"), { non_consuming = true, ignore_mods = true"));
        assert!(script.contains(
            "hl.bind(\"SHIFT + mouse:272\", hl.dsp.global(\"com.mluva.Linux:annotate-pin\")"
        ));
        assert!(!script.contains("mluva_portal_bindings"));
        assert!(remove_script(TOKEN).contains(&format!("[\"{TOKEN}\"] = nil")));
        assert!(!valid_token("../x") && !valid_token("abc"));
    }

    #[test]
    fn the_shift_release_survives_a_capital_or_a_shift_click() {
        // Without `transparent`, Hyprland 0.56 shadows the held Shift_L binding when the letter or
        // button is released, and the Shift release never reaches Mluva.
        let script = add_script(TOKEN, AnnotationModifier::Shift);
        let ink = script
            .lines()
            .find(|line| line.contains("annotate-ink"))
            .unwrap();
        assert!(ink.contains("transparent = true"));
        assert!(ink.contains("ignore_mods = true") && ink.contains("non_consuming = true"));
    }

    #[test]
    #[ignore = "requires the Lua interpreter"]
    fn lua_cleanup_of_one_session_keeps_the_next_sessions_bindings() {
        use std::{
            io::Write,
            process::{Command, Stdio},
        };
        let next = "fedcba9876543210fedcba9876543210";
        let mut lua = String::from(
            "local active = {}; local n = 0; hl = { dsp = { global = function(id) return id end }, bind = function(key, target) n = n + 1; local id = n; active[id] = key .. '=' .. target; return { remove = function() active[id] = nil end } end };\n\
             local function count() local c = 0; for _ in pairs(active) do c = c + 1 end; return c end\n",
        );
        lua.push_str(&add_script(TOKEN, AnnotationModifier::Shift));
        lua.push_str("assert(count() == 2)\n");
        // A crash left the first session's bindings; the next session replaces them.
        lua.push_str(&add_script(next, AnnotationModifier::Shift));
        lua.push_str("assert(count() == 2)\n");
        // The first session's late cleanup must not touch the next session.
        lua.push_str(&remove_script(TOKEN));
        lua.push_str("assert(count() == 2)\n");
        lua.push_str(&remove_script(next));
        lua.push_str("assert(count() == 0)\n");
        let mut child = Command::new("lua")
            .arg("-")
            .stdin(Stdio::piped())
            .spawn()
            .unwrap();
        child
            .stdin
            .take()
            .unwrap()
            .write_all(lua.as_bytes())
            .unwrap();
        assert!(child.wait().unwrap().success());
    }

    #[test]
    fn existing_shift_bindings_block_annotation_but_ours_do_not() {
        assert!(conflicting(
            &[json!({"key":"Shift_L","modmask":0,"description":"Other"})],
            AnnotationModifier::Shift
        ));
        assert!(conflicting(
            &[json!({"key":"mouse:272","modmask":1,"description":""})],
            AnnotationModifier::Shift
        ));
        assert!(!conflicting(
            &[
                json!({"key":"Shift_L","modmask":4,"description":"Other"}),
                json!({"key":"mouse:272","modmask":64,"description":"Move window"}),
                json!({"key":"Shift_L","modmask":0,"description":"Mluva annotation: annotate-ink"}),
            ],
            AnnotationModifier::Shift
        ));
    }

    #[test]
    fn outputs_use_logical_sizes_and_events_are_bounded() {
        let outputs = outputs_from(&json!([
            {"name":"DP-1","x":0,"y":0,"width":3840,"height":2160,"scale":2.0,"transform":0,"focused":true},
            {"name":"HDMI-A-1","x":1920,"y":0,"width":1920,"height":1080,"scale":1.0,"transform":1}
        ]));
        assert_eq!((outputs[0].width, outputs[0].height), (1920.0, 1080.0));
        assert_eq!((outputs[1].width, outputs[1].height), (1080.0, 1920.0));
        assert!(outputs[1].contains(2000.0, 100.0) && !outputs[0].contains(2000.0, 100.0));
        assert_eq!(
            focus_event("activewindow>>foot,private window title"),
            Some(FocusEvent::Window {
                class: "foot".into()
            })
        );
        assert_eq!(
            focus_event("workspacev2>>3,code"),
            Some(FocusEvent::Workspace {
                name: "code".into()
            })
        );
        assert_eq!(focus_event("openlayer>>mluva"), None);
    }
}
