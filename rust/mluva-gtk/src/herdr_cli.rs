//! Headless command client for the resident application's routing owner.
use glib::variant::ToVariant;
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

pub const PATH: &str = "/com/mluva/Linux/Herdr";
pub const INTERFACE: &str = "com.mluva.Linux.Herdr";
pub const XML: &str = r#"<node><interface name="com.mluva.Linux.Herdr">
<method name="Execute"><arg name="request" type="s" direction="in"/>
<arg name="result" type="s" direction="out"/></method></interface></node>"#;
const HELP: &str = "Usage: mluva herdr list [--socket PATH]
       mluva herdr status
       mluva herdr select NAME_OR_PANE [--socket PATH]
       mluva herdr clear

Prints JSON. Mluva must be running; commands never open a window or send a prompt.
Select switches the live recipient and enables routing. Clear removes the recipient.
Use --socket when names or pane IDs occur on multiple local Herdr servers.
Shortcuts and the speech provider are unchanged. Incognito selections are not saved.";

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(tag = "command", rename_all = "snake_case", deny_unknown_fields)]
pub enum Request {
    List {
        socket: Option<String>,
    },
    Status {},
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
            return Err("Choose a Herdr agent name or pane ID.");
        }
        Ok(())
    }
}
fn parse(args: &[String]) -> Result<Request, &'static str> {
    let request = match args {
        [command] if command == "status" => Request::Status {},
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
pub fn summary(recipient: &mluva_core::agent_voice::Recipient) -> Value {
    json!({"name":recipient.name,"pane":recipient.pane_id,"kind":recipient.kind,
        "project":recipient.project,"socket":recipient.socket,"status":recipient.status,
        "available":recipient.can_receive()})
}
fn call(request: &Request) -> Result<Value, String> {
    let connection = gio::bus_get_sync(gio::BusType::Session, gio::Cancellable::NONE)
        .map_err(|_| "The local session bus is unavailable.".to_owned())?;
    let request = serde_json::to_string(request).map_err(|_| "Invalid routing request.")?;
    let reply = connection.call_sync(
        Some("com.mluva.Linux"), PATH, INTERFACE, "Execute",
        Some(&(request,).to_variant()), None, gio::DBusCallFlags::NO_AUTO_START,
        40_000, gio::Cancellable::NONE,
    ).map_err(|_| "Mluva did not confirm this command. Make sure the updated app is running; use 'mluva herdr status' before retrying a switch.".to_owned())?;
    let text = reply.child_get::<String>(0);
    serde_json::from_str(&text).map_err(|_| "Mluva returned an invalid routing response.".into())
}
/// Returns None for ordinary desktop arguments. Does not initialize GTK.
pub fn run(arguments: &[String]) -> Option<u8> {
    if arguments.get(1).map(String::as_str) != Some("herdr") {
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
    }
}
