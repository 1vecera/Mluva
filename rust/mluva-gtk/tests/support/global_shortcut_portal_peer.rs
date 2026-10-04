//! Independent private-bus portal peer, also used by the unchanged release observer.
//! It does not import Mluva's shortcut implementation.
use glib::variant::{ObjectPath, ToVariant};
use serde_json::{Value, json};
use std::{
    collections::BTreeMap,
    sync::{Arc, Mutex},
};

const NAME: &str = "org.freedesktop.portal.Desktop";
const PATH: &str = "/org/freedesktop/portal/desktop";
const API: &str = "org.freedesktop.portal.GlobalShortcuts";
const REQUEST: &str = "org.freedesktop.portal.Request";
const SESSION: &str = "org.freedesktop.portal.Session";
type Dict = BTreeMap<String, glib::Variant>;

#[derive(Default)]
struct Peer {
    scenario: String,
    trace: Vec<Value>,
    clients: Vec<String>,
    sessions: Vec<String>,
    requests: Vec<String>,
    disconnected: Vec<usize>,
    held: Option<(String, Dict)>,
    payloads: Vec<glib::Variant>,
}
fn path(value: &str) -> glib::Variant {
    ObjectPath::try_from(value).unwrap().to_variant()
}
fn text(options: &Dict, key: &str) -> String {
    options[key].str().unwrap().into()
}
fn valid_token(value: &str, prefix: &str) -> bool {
    value
        .strip_prefix(prefix)
        .is_some_and(|part| part.len() == 32 && part.chars().all(|c| c.is_ascii_hexdigit()))
}
fn send(bus: &gio::DBusConnection, message: &gio::DBusMessage) {
    bus.send_message(message, gio::DBusSendMessageFlags::NONE)
        .unwrap();
}
fn signal(
    bus: &gio::DBusConnection,
    path: &str,
    interface: &str,
    member: &str,
    body: glib::Variant,
) {
    bus.emit_signal(None, path, interface, member, Some(&body))
        .unwrap();
}
fn respond(bus: &gio::DBusConnection, request: &str, code: u32, results: Dict) {
    signal(
        bus,
        request,
        REQUEST,
        "Response",
        (code, results).to_variant(),
    );
}
impl Peer {
    fn handle(
        &mut self,
        bus: &gio::DBusConnection,
        message: &gio::DBusMessage,
    ) -> Option<gio::DBusMessage> {
        let interface = message.interface().unwrap_or_default();
        let member = message.member().unwrap_or_default();
        if message.message_type() == gio::DBusMessageType::Signal && member == "NameOwnerChanged" {
            let (name, _, new) = message
                .body()
                .unwrap()
                .get::<(String, String, String)>()
                .unwrap();
            if new.is_empty()
                && let Some(index) = self.clients.iter().position(|client| *client == name)
            {
                self.disconnected.push(index + 1);
            }
            return Some(message.clone());
        }
        if message.message_type() != gio::DBusMessageType::MethodCall {
            return Some(message.clone());
        }
        if interface == "com.mluva.TestPortal" {
            let response = match member.as_str() {
                "Reset" => {
                    *self = Peer {
                        scenario: message.body().unwrap().get::<(String,)>().unwrap().0,
                        ..Self::default()
                    };
                    ().to_variant()
                }
                "Snapshot" => (serde_json::to_string(
                    &json!({"trace":self.trace,"disconnected":self.disconnected}),
                )
                .unwrap(),)
                    .to_variant(),
                "Drive" => {
                    let command = message.body().unwrap().get::<(String,)>().unwrap().0;
                    self.drive(bus, &serde_json::from_str(&command).unwrap());
                    ().to_variant()
                }
                _ => panic!("Unknown fixture control"),
            };
            let reply = message.new_method_reply();
            reply.set_body(&response);
            send(bus, &reply);
            return None;
        }
        if ![
            API,
            REQUEST,
            SESSION,
            "org.freedesktop.host.portal.Registry",
        ]
        .contains(&interface.as_str())
        {
            return Some(message.clone());
        }
        let caller = message.sender().unwrap().to_string();
        let index = if let Some(index) = self.clients.iter().position(|client| *client == caller) {
            index
        } else {
            self.clients.push(caller.clone());
            self.clients.len() - 1
        };
        let body = message.body().unwrap_or_else(|| ().to_variant());
        if member == "Register" {
            self.trace.push(json!({"method":"Register","client":index+1,"body":body.get::<(String,Dict)>().unwrap().0,"signature":body.type_().as_str()}));
            if self.scenario == "registry-error" {
                send(
                    bus,
                    &message.new_method_error_literal(
                        "org.freedesktop.DBus.Error.UnknownMethod",
                        "PRIVATE_REMOTE_TEXT_MUST_NOT_ESCAPE",
                    ),
                );
            } else {
                send(bus, &message.new_method_reply());
            }
            return None;
        }
        if member == "Close" {
            let handle = message.path().unwrap().to_string();
            let kind = if interface == SESSION {
                "session"
            } else {
                "request"
            };
            let owned = if kind == "session" {
                self.sessions.contains(&handle)
            } else {
                self.requests.contains(&handle)
            };
            self.trace
                .push(json!({"method":"Close","client":index+1,"kind":kind,"owned":owned}));
            send(bus, &message.new_method_reply());
            return None;
        }
        let options = body
            .child_value(body.n_children() - 1)
            .get::<Dict>()
            .unwrap();
        let token = text(&options, "handle_token");
        let caller_path = caller.trim_start_matches(':').replace('.', "_");
        let expected = format!("/org/freedesktop/portal/desktop/request/{caller_path}/{token}");
        let handle = if self.scenario == "alternate-early" {
            format!("{expected}_alternate")
        } else {
            expected
        };
        self.requests.push(handle.clone());
        let mut results = Dict::new();
        if member == "CreateSession" {
            let session_token = text(&options, "session_handle_token");
            let session =
                format!("/org/freedesktop/portal/desktop/session/{caller_path}/{session_token}");
            self.trace.push(json!({"method":"CreateSession","client":index+1,"signature":body.type_().as_str(),"tokens_valid":valid_token(&token,"mluva_")&&valid_token(&session_token,"mluva_session_")}));
            self.sessions.push(session.clone());
            if self.scenario == "method-error" {
                send(
                    bus,
                    &message.new_method_error_literal(
                        "org.freedesktop.portal.Error.NotAllowed",
                        "PRIVATE_REMOTE_TEXT_MUST_NOT_ESCAPE",
                    ),
                );
                return None;
            }
            match self.scenario.as_str() {
                "missing-session" => {}
                "bad-session" => {
                    results.insert(
                        "session_handle".into(),
                        "/org/freedesktop/portal/desktop/session/foreign/token".to_variant(),
                    );
                }
                _ => {
                    results.insert("session_handle".into(), session.to_variant());
                }
            }
        } else if member == "BindShortcuts" {
            let selected = body.child_value(0);
            let payload = body.child_value(1);
            let proposals = payload.get::<Vec<(String, Dict)>>().unwrap();
            self.payloads.push(payload.clone());
            self.trace.push(json!({"method":"BindShortcuts","client":index+1,"signature":body.type_().as_str(),"session_owned":self.sessions.last().unwrap()==selected.str().unwrap(),"parent":body.child_value(2).str().unwrap(),"token_valid":valid_token(&token,"mluva_"),"shortcuts":proposals.iter().map(|(id,options)|json!({"id":id,"description":text(options,"description"),"trigger":text(options,"preferred_trigger")})).collect::<Vec<_>>()}));
            let approved = proposals
                .iter()
                .map(|(id, options)| {
                    (
                        id,
                        Dict::from([
                            ("description".into(), options["description"].clone()),
                            (
                                "trigger_description".into(),
                                options["preferred_trigger"].clone(),
                            ),
                        ]),
                    )
                })
                .collect::<Vec<_>>()
                .to_variant();
            match self.scenario.as_str() {
                "missing-shortcuts" => {}
                "invalid-shortcuts" => {
                    results.insert("shortcuts".into(), "invalid".to_variant());
                }
                "dictionary-shortcuts" => {
                    results.insert("shortcuts".into(), Dict::new().to_variant());
                }
                "invalid-description" => {
                    results.insert(
                        "shortcuts".into(),
                        vec![(
                            "toggle-recording-f9",
                            Dict::from([("description".into(), 1u32.to_variant())]),
                        )]
                        .to_variant(),
                    );
                }
                _ => {
                    results.insert("shortcuts".into(), approved);
                }
            }
        } else {
            panic!("Unexpected portal member {member}");
        }
        let reply = message.new_method_reply();
        reply.set_body(&match self.scenario.as_str() {
            "bad-request" => (2u32,).to_variant(),
            "foreign-request" => glib::Variant::tuple_from_iter([path(
                "/org/freedesktop/portal/desktop/request/foreign/token",
            )]),
            _ => glib::Variant::tuple_from_iter([path(&handle)]),
        });
        let code = if self.scenario == "cancel-create" {
            1
        } else if self.scenario == "deny-bind" && member == "BindShortcuts" {
            2
        } else {
            0
        };
        let pending = (self.scenario == "pending-create" && member == "CreateSession")
            || (["pending-bind", "queued-key"].contains(&self.scenario.as_str())
                && member == "BindShortcuts"
                && self.payloads.len() == 1);
        let early = ["early", "alternate-early"].contains(&self.scenario.as_str());
        if early {
            respond(bus, &handle, code, results.clone());
        }
        send(bus, &reply);
        if pending {
            self.held = Some((handle, results));
        } else if self.scenario == "invalid-response" {
            signal(bus, &handle, REQUEST, "Response", ("bad",).to_variant());
        } else if !early {
            respond(bus, &handle, code, results);
        }
        None
    }

    fn drive(&mut self, bus: &gio::DBusConnection, command: &Value) {
        let name = command["op"].as_str().unwrap();
        if name == "approve" {
            let (handle, results) = self.held.take().unwrap();
            respond(bus, &handle, 0, results);
            return;
        }
        let selected = command
            .get("session")
            .and_then(Value::as_u64)
            .map(|index| index as usize - 1)
            .unwrap_or(self.sessions.len() - 1);
        let session = if command["foreign"].as_bool() == Some(true) {
            "/org/freedesktop/portal/desktop/session/foreign/token"
        } else {
            &self.sessions[selected]
        };
        match name {
            "Activated" | "Deactivated" => signal(
                bus,
                PATH,
                API,
                name,
                glib::Variant::tuple_from_iter([
                    path(session),
                    command["id"].as_str().unwrap().to_variant(),
                    1u64.to_variant(),
                    Dict::new().to_variant(),
                ]),
            ),
            "malformed-activation" => signal(bus, PATH, API, "Activated", (session,).to_variant()),
            "untyped-activation" => signal(
                bus,
                PATH,
                API,
                "Activated",
                (0u32, command["id"].as_str().unwrap(), 1u64, Dict::new()).to_variant(),
            ),
            "closed" => signal(bus, session, SESSION, "Closed", ().to_variant()),
            "changed" => {
                let payload = command["shortcuts"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .map(|item| {
                        (
                            item["id"].as_str().unwrap(),
                            Dict::from([(
                                "trigger_description".into(),
                                item["trigger"].as_str().unwrap().to_variant(),
                            )]),
                        )
                    })
                    .collect::<Vec<_>>()
                    .to_variant();
                signal(
                    bus,
                    PATH,
                    API,
                    "ShortcutsChanged",
                    glib::Variant::tuple_from_iter([path(session), payload]),
                );
            }
            "malformed-change" => signal(
                bus,
                PATH,
                API,
                "ShortcutsChanged",
                glib::Variant::tuple_from_iter([path(session), "bad".to_variant()]),
            ),
            _ => panic!("Unknown signal command"),
        }
    }
}

fn main() {
    let root = std::env::var("OFFSCREEN_SESSION_ROOT").expect("private runner required");
    assert!(std::env::var("HOME").unwrap().starts_with(&root));
    let address = std::env::var("DBUS_SESSION_BUS_ADDRESS").unwrap();
    assert_ne!(
        std::fs::read_link("/proc/self/ns/net")
            .unwrap()
            .to_str()
            .unwrap(),
        std::env::var("MLUVA_HOST_NET_NS").unwrap()
    );
    for device in ["/dev/input", "/dev/uinput", "/dev/snd", "/dev/dri"] {
        assert!(!std::path::Path::new(device).exists());
    }
    let bus = gio::DBusConnection::for_address_sync(
        &address,
        gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
            | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
        None,
        None::<&gio::Cancellable>,
    )
    .unwrap();
    bus.set_exit_on_close(false);
    let peer = Arc::new(Mutex::new(Peer::default()));
    let filter = bus.add_filter(move |bus, message, incoming| {
        if incoming {
            peer.lock().unwrap().handle(bus, message)
        } else {
            Some(message.clone())
        }
    });
    let owned = bus
        .call_sync(
            Some("org.freedesktop.DBus"),
            "/org/freedesktop/DBus",
            "org.freedesktop.DBus",
            "RequestName",
            Some(&(NAME, 4u32).to_variant()),
            None,
            gio::DBusCallFlags::NONE,
            2000,
            None::<&gio::Cancellable>,
        )
        .unwrap();
    assert_eq!(
        owned.get::<(u32,)>().unwrap().0,
        1,
        "private portal name already owned"
    );
    bus.call_sync(Some("org.freedesktop.DBus"),"/org/freedesktop/DBus","org.freedesktop.DBus","AddMatch",Some(&("type='signal',sender='org.freedesktop.DBus',interface='org.freedesktop.DBus',member='NameOwnerChanged'",).to_variant()),None,gio::DBusCallFlags::NONE,2000,None::<&gio::Cancellable>).unwrap();
    println!("portal-ready");
    glib::MainLoop::new(None, false).run();
    bus.remove_filter(filter);
}
