use super::*;
use crate::herdr_cli::{INTERFACE, PATH, Request, XML, summary};
use glib::variant::ToVariant;
use serde_json::{Value, json};

impl AgentVoiceController {
    pub fn register_cli(
        self: &Rc<Self>,
        connection: &gio::DBusConnection,
    ) -> Result<(), glib::Error> {
        let info = gio::DBusNodeInfo::for_xml(XML)?
            .lookup_interface(INTERFACE)
            .expect("static interface");
        let weak = Rc::downgrade(self);
        let registration = connection
            .register_object(PATH, &info)
            .method_call(move |_, _, _, _, method, parameters, invocation| {
                let Some(owner) = weak.upgrade().filter(|owner| !owner.closed.is_cancelled())
                else {
                    invocation.return_dbus_error("com.mluva.Linux.Closed", "Mluva is closing.");
                    return;
                };
                let text = parameters.child_get::<String>(0);
                let request = (method == "Execute" && text.len() <= 4096)
                    .then(|| serde_json::from_str::<Request>(&text).ok())
                    .flatten();
                let Some(request) = request.filter(|request| request.validate().is_ok()) else {
                    invocation
                        .return_dbus_error("com.mluva.Linux.Invalid", "Invalid routing command.");
                    return;
                };
                owner.runtime.clone().spawn(async move {
                    let result = owner
                        .cli_request(request)
                        .await
                        .unwrap_or_else(|error| json!({"ok":false,"error":error}));
                    invocation.return_value(Some(&(result.to_string(),).to_variant()));
                });
            })
            .build()?;
        self.cli.replace(Some((connection.clone(), registration)));
        Ok(())
    }

    async fn cli_discover(&self, socket: Option<&str>) -> (Vec<Recipient>, Vec<String>) {
        let paths = herdr::socket_candidates(
            self.services
                .paths
                .config
                .parent()
                .unwrap_or(Path::new("/")),
            socket,
        );
        let mut recipients = vec![];
        let mut errors = vec![];
        for path in paths.into_iter().take(16) {
            let result = tokio::select! {
                _ = self.closed.cancelled() => break,
                result = herdr::discover(&path) => result,
            };
            match result {
                Ok(found) => recipients.extend(found.recipients),
                Err(error) => errors.push(error.to_string()),
            }
        }
        (recipients, errors)
    }
    async fn cli_request(self: &Rc<Self>, request: Request) -> Result<Value, String> {
        let generation = self.generation.get();
        match request {
            Request::List { socket } => {
                let (recipients, errors) = self.cli_discover(socket.as_deref()).await;
                Ok(
                    json!({"ok":true,"recipients":recipients.iter().map(summary).collect::<Vec<_>>(),"errors":errors}),
                )
            }
            Request::Status {} => {
                let settings = self.settings();
                let socket = settings
                    .selected
                    .as_ref()
                    .map(|r| r.socket.as_str())
                    .or(settings.socket.as_deref());
                let (recipients, errors) = self.cli_discover(socket).await;
                if self.closed.is_cancelled() || self.generation.get() != generation {
                    return Err("Routing changed during discovery. Check status again.".into());
                }
                let live = settings
                    .selected
                    .as_ref()
                    .and_then(|s| recipients.iter().find(|r| r.same_instance(s)));
                let selected = live.or(settings.selected.as_ref()).map(|recipient| {
                    let mut value = summary(recipient);
                    value["available"] = json!(live.is_some_and(Recipient::can_receive));
                    value
                });
                Ok(json!({"ok":true,"running":true,"enabled":settings.enabled,
                    "selected":selected,
                    "connected":settings.enabled && live.is_some_and(Recipient::can_receive),
                    "hold_key":settings.hold_key,"draft_key":settings.draft_key,
                    "incognito":self.services.config().incognito_mode,"errors":errors}))
            }
            Request::Select { target, socket } => {
                // The user's switch intent must cancel a pending send before discovery waits.
                self.disarm(DraftReason::RecipientChanged);
                let (recipients, _) = self.cli_discover(socket.as_deref()).await;
                if self.closed.is_cancelled() || self.generation.get() != generation {
                    return Err(
                        "Routing changed during discovery. Check status and try again.".into(),
                    );
                }
                let matches: Vec<_> = recipients
                    .iter()
                    .filter(|r| r.name == target || r.pane_id == target)
                    .collect();
                if matches.len() > 1 {
                    return Err(
                        "Multiple recipients match. Use an exact pane ID and, if needed, --socket."
                            .into(),
                    );
                }
                let selected = matches.first().filter(|r| r.can_receive())
                    .ok_or("No matching available guarded recipient. Use 'mluva herdr list' to check the session.")?;
                self.cli_select(Some((*selected).clone()))?;
                self.connection
                    .replace("Local Herdr · release to finish, then two seconds to cancel".into());
                self.update_recipients(recipients);
                Ok(
                    json!({"ok":true,"connected":true,"selected":self.settings().selected.as_ref().map(summary),
                    "persisted":!self.services.config().incognito_mode}),
                )
            }
            Request::Clear {} => {
                self.cli_select(None)?;
                let recipients = self.recipients.borrow().clone();
                self.update_recipients(recipients);
                Ok(
                    json!({"ok":true,"connected":false,"selected":null,"persisted":!self.services.config().incognito_mode}),
                )
            }
        }
    }
    fn cli_select(self: &Rc<Self>, selected: Option<Recipient>) -> Result<(), String> {
        if self.closed.is_cancelled() {
            return Err("Mluva is closing.".into());
        }
        let mut proposed = self.settings();
        let enabled_before = proposed.enabled;
        if let Some(recipient) = &selected {
            proposed.enabled = true;
            proposed.socket = Some(recipient.socket.clone());
        }
        proposed.selected = selected;
        if !self.services.config().incognito_mode {
            proposed.save(
                &self.services.paths.config.join("agent-voice.json"),
                &self.services.config().global_recording_key,
            )?;
        }
        self.disarm(DraftReason::RecipientChanged);
        self.stop_voice();
        self.settings.replace(proposed.clone());
        self.generation.set(self.generation.get() + 1);
        self.syncing.set(true);
        self.view.enabled.set_active(proposed.enabled);
        self.view
            .socket
            .set_text(proposed.socket.as_deref().unwrap_or(""));
        self.view.reply.set_label("");
        self.syncing.set(false);
        if enabled_before != proposed.enabled {
            (self.callbacks.settings_changed)();
        }
        self.render();
        Ok(())
    }
}
