//! Headless command client for the resident application's routing owner.
use glib::variant::ToVariant;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PATH: &str = "/com/mluva/Linux/Herdr";
pub const INTERFACE: &str = "com.mluva.Linux.Herdr";
pub const XML: &str = r#"<node><interface name="com.mluva.Linux.Herdr">
<method name="Execute"><arg name="request" type="s" direction="in"/>
<arg name="result" type="s" direction="out"/></method></interface></node>"#;
const HELP: &str = "Usage: mluva agents list [--socket PATH]
       mluva agents status
       mluva agents select NAME_PANE_OR_SESSION [--socket PATH]
       mluva agents clear
       mluva agents record

`mluva herdr` is the same command.
Prints JSON. Mluva must be running; commands never open a window.
Record toggles dictation and delivers its final text to the selected agent: sent,
queued or placed in its composer, as each session's `delivery` says.
List, status, select and clear never deliver anything.
Select switches the live recipient and enables routing; it accepts only sessions Mluva
can deliver to. Clear removes the recipient.
Use --socket when names or pane IDs occur on multiple local Herdr servers.
Shortcuts and the speech provider are unchanged. Incognito selections are not saved.";

/// The command names that reach the routing owner: `agents`, and `herdr` as before.
const COMMANDS: [&str; 2] = ["agents", "herdr"];

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List {
        socket: Option<String>,
    },
    Status {},
    Record {},
    Select {
        target: String,
        socket: Option<String>,
    },
    Clear {},
}
impl Request {
    pub fn validate(&self) -> Result<(), &'static str> {
        let socket = match self {
            Self::List { socket } | Self::Select { socket, .. } => socket.as_deref(),
            _ => None,
        };
        if socket.is_some_and(|s| !s.starts_with('/') || s.len() > 103 || s.contains('\0')) {
            return Err("Use an absolute local Herdr socket path of at most 103 bytes.");
        }
        if let Self::Select { target, .. } = self
            && (target.trim().is_empty()
                || target.len() > 128
                || target.chars().any(char::is_control))
        {
            return Err("Choose an agent name, pane or session ID.");
        }
        Ok(())
    }
}
fn parse(args: &[String]) -> Result<Request, &'static str> {
    let request = match args {
        [command] if command == "status" => Request::Status {},
        [command] if command == "record" => Request::Record {},
        [command] if command == "clear" => Request::Clear {},
        [command] if command == "list" => Request::List { socket: None },
        [command, flag, socket] if command == "list" && flag == "--socket" => Request::List {
            socket: Some(socket.clone()),
        },
        [command, target] if command == "select" => Request::Select {
            target: target.clone(),
            socket: None,
        },
        [command, target, flag, socket] if command == "select" && flag == "--socket" => {
            Request::Select {
                target: target.clone(),
                socket: Some(socket.clone()),
            }
        }
        _ => return Err(HELP),
    };
    request.validate()?;
    Ok(request)
}
/// One target as the command line reports it. `display_name` is the name the wheel shows (the
/// session's own title, else its handle); `name` is the handle `select` matches. `pane` and
/// `socket` are Herdr's and empty elsewhere; `route` names the transport and `delivery` what
/// recording does.
pub fn summary(target: &mluva_core::agent_voice::Target) -> Value {
    let pane = target.herdr.clone().unwrap_or_default();
    json!({"name":target.name,"title":target.title,"display_name":target.display_name(),
        "pane":pane.pane_id,"kind":target.kind,
        "project":target.project,"socket":pane.socket,"status":target.status,
        "session":target.session_id,"route":target.route().as_str(),
        "delivery":target.delivery(),"available":target.can_receive()})
}
fn call(request: &Request) -> Result<Value, String> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|_| "The local session bus is unavailable.".to_owned())?;
    let request = serde_json::to_string(request).map_err(|_| "Invalid routing request.")?;
    let reply = connection.call_sync(
        Some("com.mluva.Linux"), PATH, INTERFACE, "Execute",
        Some(&(request,).to_variant()), None, gio::DBusCallFlags::NO_AUTO_START,
        40_000, gio::Cancellable::NONE,
    ).map_err(|_| "Mluva did not confirm this command. Make sure the updated app is running; use 'mluva agents status' before retrying a switch.".to_owned())?;
    let text = reply.child_get::<String>(0);
    serde_json::from_str(&text).map_err(|_| "Mluva returned an invalid routing response.".into())
}
/// Returns None for ordinary desktop arguments. Does not initialize GTK.
pub fn run(arguments: &[String]) -> Option<u8> {
    if !arguments
        .get(1)
        .is_some_and(|command| COMMANDS.contains(&command.as_str()))
    {
        return None;
    }
    let args = &arguments[2..];
    if args.is_empty() || matches!(args, [flag] if flag == "--help" || flag == "-h") {
        println!("{HELP}");
        return Some(0);
    }
    let result = parse(args)
        .map_err(str::to_owned)
        .and_then(|request| call(&request));
    let value = result.unwrap_or_else(|error| json!({"ok":false,"error":error}));
    println!(
        "{}",
        serde_json::to_string_pretty(&value).expect("JSON value")
    );
    Some(if value["ok"] == true { 0 } else { 1 })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn malformed_switches_never_reach_the_resident() {
        for args in [
            vec!["select"],
            vec!["select", ""],
            vec!["select", "cos", "--socket", "relative"],
            vec!["status", "--socket", "/tmp/a"],
            vec!["clear", "cos"],
        ] {
            assert!(parse(&args.into_iter().map(str::to_owned).collect::<Vec<_>>()).is_err());
        }
        assert!(matches!(
            parse(&["select".into(), "cos".into()]),
            Ok(Request::Select { socket: None, .. })
        ));
        assert!(
            serde_json::from_str::<Request>(r#"{"command":"clear","unexpected":true}"#).is_err()
        );
        // A Codex thread or Claude Code session is chosen by its ID.
        assert!(matches!(
            parse(&[
                "select".into(),
                "0199a1b2-c3d4-7e5f-8a9b-0c1d2e3f4a5b".into()
            ]),
            Ok(Request::Select { .. })
        ));
    }

    #[test]
    fn both_command_names_reach_the_routing_owner_and_nothing_else_does() {
        assert_eq!(run(&["mluva".into(), "--version".into()]), None);
        assert_eq!(run(&["mluva".into()]), None);
        for command in COMMANDS {
            // Help is answered locally, without the session bus.
            assert_eq!(
                run(&["mluva".into(), command.into(), "--help".into()]),
                Some(0)
            );
        }
    }

    #[test]
    fn the_summary_says_which_route_and_delivery_a_session_has() {
        use mluva_core::agent_voice::{AgentKind, AgentStatus, HerdrPane, Target};
        let outside = Target {
            name: "notes-3f".into(),
            title: "Portal test".into(),
            kind: AgentKind::ClaudeCode,
            status: AgentStatus::Idle,
            session_id: "fixture-session".into(),
            pid: 4_100,
            ..Target::default()
        };
        let value = summary(&outside);
        // People read the title; `select` matches the handle.
        assert_eq!(
            (&value["display_name"], &value["name"]),
            (&json!("Portal test"), &json!("notes-3f"))
        );
        assert_eq!(
            (
                &value["route"],
                &value["delivery"],
                &value["available"],
                &value["pane"],
                &value["kind"]
            ),
            (
                &json!("none"),
                &json!("draft_only"),
                &json!(false),
                &json!(""),
                &json!("claude")
            )
        );
        let guarded = Target {
            herdr: Some(HerdrPane {
                socket: "/run/user/fixture/herdr.sock".into(),
                server_epoch: "epoch".into(),
                pane_id: "w1:p2".into(),
                guard: "a".repeat(64),
            }),
            kind: AgentKind::CodexCli,
            ..outside
        };
        let value = summary(&guarded);
        assert_eq!(
            (
                &value["route"],
                &value["delivery"],
                &value["available"],
                &value["pane"],
                &value["kind"]
            ),
            (
                &json!("herdr"),
                &json!("send"),
                &json!(true),
                &json!("w1:p2"),
                &json!("codex")
            )
        );
        let agy = Target {
            name: "agy".into(),
            title: "Add Agy Support".into(),
            kind: AgentKind::AntigravityCli,
            status: AgentStatus::Idle,
            session_id: "agy-conv-1".into(),
            pid: 5_000,
            antigravity: Some(mluva_core::agent_voice::AntigravitySession {
                conversation_id: "agy-conv-1".into(),
                lock: "/fake/agy.lock".into(),
                holder: 5_000,
                proc_start: "12345".into(),
            }),
            ..Target::default()
        };
        let value = summary(&agy);
        assert_eq!(
            (
                &value["display_name"],
                &value["route"],
                &value["delivery"],
                &value["available"],
                &value["kind"]
            ),
            (
                &json!("Add Agy Support"),
                &json!("antigravity_queue"),
                &json!("queue"),
                &json!(true),
                &json!("agy")
            )
        );
    }
}
