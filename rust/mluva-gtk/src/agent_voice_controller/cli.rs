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

    async fn cli_discover(&self, socket: Option<&str>) -> (Vec<Target>, Vec<String>) {
        let sources = self.sources(socket);
        let cancel = self.closed.clone();
        self.runtime
            .spawn_background(async move { agent_targets::discover(sources, &cancel).await })
            .await
            .ok()
            .flatten()
            .map(|found| (found.targets, found.errors))
            .unwrap_or_default()
    }
    async fn cli_request(self: &Rc<Self>, request: Request) -> Result<Value, String> {
        let generation = self.generation.get();
        match request {
            Request::List { socket } => {
                let (targets, errors) = self.cli_discover(socket.as_deref()).await;
                Ok(
                    json!({"ok":true,"recipients":targets.iter().map(summary).collect::<Vec<_>>(),"errors":errors}),
                )
            }
            Request::Record {} => {
                let recording = self
                    .route
                    .borrow()
                    .attempt
                    .as_ref()
                    .is_some_and(|a| a.phase == RoutePhase::Recording);
                if !recording && (self.callbacks.capture_busy)() {
                    return Err("Capture is not ready. Check Mluva's speech settings or finish active work.".into());
                }
                if !recording
                    && (!self.settings().enabled
                        || !self.selected().is_some_and(|t| t.can_receive()))
                {
                    return Err("Choose an available agent before recording.".into());
                }
                (self.callbacks.toggle)();
                Ok(json!({"ok":true,"delivery":self.delivery_status()}))
            }
            Request::Status {} => {
                let settings = self.settings();
                // A session outside Herdr has no socket of its own to check.
                let socket = settings
                    .selected
                    .as_ref()
                    .and_then(|t| t.herdr.as_ref())
                    .map(|pane| pane.socket.as_str())
                    .filter(|socket| !socket.is_empty())
                    .or(settings.socket.as_deref());
                let (targets, errors) = self.cli_discover(socket).await;
                if self.closed.is_cancelled() || self.generation.get() != generation {
                    return Err("Routing changed during discovery. Check status again.".into());
                }
                let live = settings
                    .selected
                    .as_ref()
                    .and_then(|s| targets.iter().find(|t| t.same_instance(s)));
                let selected = live.or(settings.selected.as_ref()).map(|target| {
                    let mut value = summary(target);
                    value["available"] = json!(live.is_some_and(Target::can_receive));
                    value
                });
                Ok(json!({"ok":true,"running":true,"enabled":settings.enabled,
                    "selected":selected,
                    "connected":settings.enabled && live.is_some_and(Target::can_receive),
                    "hold_key":settings.hold_key,"draft_key":settings.draft_key,"shortcuts":self.page.shortcut_state(),
                    "agent_shortcut":{"preferred":settings.hold_key,"assigned":self.approved_key.borrow().clone(),"ready":self.approved_key.borrow().is_some()},
                    "delivery":self.delivery_status(),"incognito":self.services.config().incognito_mode,"errors":errors}))
            }
            Request::Select { target, socket } => {
                // The user's switch intent must cancel a pending send before discovery waits.
                self.disarm(DraftReason::RecipientChanged);
                let (targets, _) = self.cli_discover(socket.as_deref()).await;
                if self.closed.is_cancelled() || self.generation.get() != generation {
                    return Err(
                        "Routing changed during discovery. Check status and try again.".into(),
                    );
                }
                let matches: Vec<_> = targets.iter().filter(|t| t.answers_to(&target)).collect();
                if matches.len() > 1 {
                    return Err(
                        "Multiple recipients match. Use an exact pane or session ID and, if needed, --socket."
                            .into(),
                    );
                }
                let selected = matches.first().filter(|t| t.can_receive())
                    .ok_or("No matching recipient Mluva can deliver to. Use 'mluva agents list' to check the session.")?;
                self.cli_select(Some((*selected).clone()))?;
                self.connection
                    .replace("Local agents · press to start and again to stop and deliver".into());
                self.update_targets(targets);
                Ok(
                    json!({"ok":true,"connected":true,"selected":self.settings().selected.as_ref().map(summary),
                    "persisted":!self.services.config().incognito_mode}),
                )
            }
            Request::Clear {} => {
                self.cli_select(None)?;
                let targets = self.targets.borrow().clone();
                self.update_targets(targets);
                Ok(
                    json!({"ok":true,"connected":false,"selected":null,"persisted":!self.services.config().incognito_mode}),
                )
            }
        }
    }
    fn delivery_status(&self) -> &'static str {
        self.route
            .borrow()
            .attempt
            .as_ref()
            .map_or("idle", |a| match a.phase {
                RoutePhase::Recording => "recording",
                RoutePhase::Finalizing => "transcribing",
                RoutePhase::Grace { .. } => "countdown",
                RoutePhase::Submitting => "sending",
                RoutePhase::Delivered | RoutePhase::Held => a.delivered_outcome().unwrap_or("sent"),
                RoutePhase::Unconfirmed => "unconfirmed",
                RoutePhase::Draft(_) => "draft",
            })
    }
    fn cli_select(self: &Rc<Self>, selected: Option<Target>) -> Result<(), String> {
        if self.closed.is_cancelled() {
            return Err("Mluva is closing.".into());
        }
        let mut proposed = self.settings();
        let enabled_before = proposed.enabled;
        if let Some(target) = &selected {
            proposed.enabled = true;
            // A Herdr session pins its server; any other keeps the configured socket.
            if let Some(pane) = &target.herdr {
                proposed.socket = Some(pane.socket.clone());
            }
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
