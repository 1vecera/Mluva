use super::*;

type Response = Result<(u32, Properties)>;
type Pending = Rc<RefCell<Option<oneshot::Sender<Response>>>>;

pub(super) struct PortalSession {
    key: String,
    recording_id: String,
    callbacks: ShortcutCallbacks,
    bus: RefCell<Option<gio::DBusConnection>>,
    subscriptions: RefCell<Vec<gio::SignalSubscription>>,
    handle: RefCell<Option<String>>,
    pending: RefCell<BTreeMap<String, Pending>>,
    early: RefCell<BTreeMap<String, Response>>,
    active_requests: RefCell<BTreeSet<String>>,
    pressed: RefCell<BTreeSet<String>>,
    pub cancel: CancellationToken,
    cleanup: Mutex<()>,
}
impl PortalSession {
    pub fn new(key: &str, callbacks: ShortcutCallbacks) -> Rc<Self> {
        Rc::new(Self {
            key: key.into(),
            recording_id: recording_shortcut_id(key).expect("validated recording key"),
            callbacks,
            bus: RefCell::new(None),
            subscriptions: RefCell::new(vec![]),
            handle: RefCell::new(None),
            pending: RefCell::new(BTreeMap::new()),
            early: RefCell::new(BTreeMap::new()),
            active_requests: RefCell::new(BTreeSet::new()),
            pressed: RefCell::new(BTreeSet::new()),
            cancel: CancellationToken::new(),
            cleanup: Mutex::new(()),
        })
    }

    pub async fn connect(self: &Rc<Self>) -> Result<Vec<BoundShortcut>> {
        let address =
            gio::dbus_address_get_for_bus_sync(gio::BusType::Session, None::<&gio::Cancellable>)
                .map_err(|_| PortalError::from("The global shortcut portal is not connected."))?;
        let connection = gio::DBusConnection::for_address_future(
            &address,
            gio::DBusConnectionFlags::AUTHENTICATION_CLIENT
                | gio::DBusConnectionFlags::MESSAGE_BUS_CONNECTION,
            None,
        );
        let bus = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(CLOSED.into()),
            result = connection => result.map_err(|_| PortalError::from("The global shortcut portal is not connected."))?,
        };
        bus.set_exit_on_close(false);
        *self.bus.borrow_mut() = Some(bus.clone());
        if std::env::var_os("FLATPAK_ID").is_none_or(|value| value.is_empty())
            && !std::path::Path::new("/.flatpak-info").exists()
        {
            let _ = self
                .call(
                    PORTAL,
                    PATH,
                    "org.freedesktop.host.portal.Registry",
                    "Register",
                    &("com.mluva.Linux", Properties::new()).to_variant(),
                    "Host application registration",
                )
                .await;
        }
        // Explicitly await AddMatch on this connection. A portal is allowed to
        // emit Response before returning the request handle.
        for (interface, member, path) in [
            (REQUEST, Some("Response"), None),
            (SHORTCUTS, None, Some(PATH)),
            (SESSION, Some("Closed"), None),
        ] {
            let weak = Rc::downgrade(self);
            self.subscriptions
                .borrow_mut()
                .push(bus.subscribe_to_signal(
                    Some(PORTAL),
                    Some(interface),
                    member,
                    path,
                    None,
                    gio::DBusSignalFlags::NO_MATCH_RULE,
                    move |signal| {
                        if let Some(owner) = weak.upgrade() {
                            owner.signal(
                                signal.interface_name,
                                signal.signal_name,
                                signal.object_path,
                                signal.parameters,
                            );
                        }
                    },
                ));
            let mut rule = format!("type='signal',sender='{PORTAL}',interface='{interface}'");
            if let Some(member) = member {
                rule.push_str(&format!(",member='{member}'"));
            }
            if let Some(path) = path {
                rule.push_str(&format!(",path='{path}'"));
            }
            self.call(
                "org.freedesktop.DBus",
                "/org/freedesktop/DBus",
                "org.freedesktop.DBus",
                "AddMatch",
                &(rule,).to_variant(),
                "D-Bus signal subscription",
            )
            .await?;
        }
        let token = token("mluva_session_");
        let created = self
            .request(
                "CreateSession",
                vec![],
                Properties::from([("session_handle_token".into(), token.to_variant())]),
            )
            .await?;
        let handle = created
            .get("session_handle")
            .and_then(|value| value.str())
            .ok_or_else(|| {
                PortalError::from("The portal did not return a valid shortcut session handle.")
            })?;
        let expected = format!("{SESSION_PATH}/{}/{token}", sender(&bus)?);
        if handle != expected {
            return Err("The portal returned an unexpected shortcut session handle.".into());
        }
        *self.handle.borrow_mut() = Some(handle.into());
        let payload = [
            (
                self.recording_id.as_str(),
                format!("Start or stop Mluva recording with {}", self.key),
                self.key.as_str(),
            ),
            (
                CANCEL,
                "Cancel active Mluva capture".into(),
                "CTRL+ALT+ESCAPE",
            ),
            (
                REWRITE,
                "Open the latest Mluva conversation for rewriting".into(),
                "SHIFT+F9",
            ),
        ]
        .into_iter()
        .map(|(id, description, trigger)| {
            (
                id,
                Properties::from([
                    ("description".into(), description.to_variant()),
                    ("preferred_trigger".into(), trigger.to_variant()),
                ]),
            )
        })
        .collect::<Vec<_>>();
        let bound = self
            .request(
                "BindShortcuts",
                vec![
                    glib::variant::ObjectPath::try_from(handle)
                        .expect("validated owned path")
                        .to_variant(),
                    payload.to_variant(),
                    "".to_variant(),
                ],
                Properties::new(),
            )
            .await?;
        bound_shortcuts(bound.get("shortcuts").ok_or_else(|| {
            PortalError::from("The portal did not return its approved shortcuts.")
        })?)
    }

    pub fn report_bindings(&self, bound: &[BoundShortcut]) {
        let trigger = |id: &str| {
            bound
                .iter()
                .find(|value| value.id == id)
                .map(|value| value.trigger.trim_matches(mluva_core::text::whitespace))
                .filter(|value| !value.is_empty())
        };
        (self.callbacks.binding_changed)(&self.key, trigger(&self.recording_id));
        (self.callbacks.rewrite_binding_changed)(trigger(REWRITE));
    }

    async fn call(
        &self,
        destination: &str,
        path: &str,
        interface: &str,
        member: &str,
        body: &glib::Variant,
        operation: &str,
    ) -> Result<gio::DBusMessage> {
        let bus = self
            .bus
            .borrow()
            .clone()
            .ok_or_else(|| PortalError::from("The global shortcut portal is not connected."))?;
        let message =
            gio::DBusMessage::new_method_call(Some(destination), path, Some(interface), member);
        message.set_body(body);
        let result = tokio::select! {
            biased;
            _ = self.cancel.cancelled() => return Err(CLOSED.into()),
            result = bus.send_message_with_reply_future(&message, gio::DBusSendMessageFlags::NONE, -1) => result,
        };
        let reply =
            result.map_err(|_| PortalError(format!("{operation} returned no D-Bus reply.")))?;
        match reply.message_type() {
            gio::DBusMessageType::MethodReturn => Ok(reply),
            gio::DBusMessageType::Error => Err(PortalError(format!(
                "{operation} failed ({}).",
                reply
                    .error_name()
                    .as_deref()
                    .unwrap_or("unknown D-Bus error")
            ))),
            _ => Err(PortalError(format!(
                "{operation} returned an unexpected D-Bus message."
            ))),
        }
    }

    async fn request(
        &self,
        member: &str,
        mut positional: Vec<glib::Variant>,
        mut options: Properties,
    ) -> Result<Properties> {
        let bus = self
            .bus
            .borrow()
            .clone()
            .ok_or_else(|| PortalError::from("The global shortcut portal is not connected."))?;
        let prefix = format!("{REQUEST_PATH}/{}/", sender(&bus)?);
        let token = token("mluva_");
        let expected = format!("{prefix}{token}");
        let mut returned = expected.clone();
        options.insert("handle_token".into(), token.to_variant());
        positional.push(options.to_variant());
        let (send, receive) = oneshot::channel();
        let pending = Rc::new(RefCell::new(Some(send)));
        self.pending
            .borrow_mut()
            .insert(expected.clone(), pending.clone());
        self.active_requests.borrow_mut().insert(expected.clone());
        let outcome = async {
            let reply = self
                .call(
                    PORTAL,
                    PATH,
                    SHORTCUTS,
                    member,
                    &glib::Variant::tuple_from_iter(positional),
                    &format!("Global shortcut {member}"),
                )
                .await?;
            let body = reply.body().filter(|body| body.n_children() == 1);
            let path = body.as_ref().map(|body| body.child_value(0));
            returned = path
                .as_ref()
                .and_then(|path| path.str())
                .ok_or_else(|| {
                    PortalError(format!(
                        "Global shortcut {member} returned an invalid request handle."
                    ))
                })?
                .into();
            if !returned.starts_with(&prefix)
                || glib::variant::ObjectPath::try_from(returned.as_str()).is_err()
            {
                return Err(PortalError(format!(
                    "Global shortcut {member} returned an unexpected request handle."
                )));
            }
            if returned != expected && pending.borrow().is_some() {
                self.pending.borrow_mut().remove(&expected);
                self.active_requests.borrow_mut().remove(&expected);
                self.pending
                    .borrow_mut()
                    .insert(returned.clone(), pending.clone());
                self.active_requests.borrow_mut().insert(returned.clone());
                if let Some(response) = self.early.borrow_mut().remove(&returned)
                    && let Some(send) = pending.borrow_mut().take()
                {
                    let _ = send.send(response);
                }
            }
            let (code, results) = tokio::select! {
                biased;
                _ = self.cancel.cancelled() => return Err(CLOSED.into()),
                response = receive => response.map_err(|_| PortalError::from(CLOSED))??,
            };
            match code {
                0 => Ok(results),
                1 => Err("Global shortcut approval was cancelled.".into()),
                _ => Err("The desktop could not complete global shortcut approval.".into()),
            }
        }
        .await;
        self.pending
            .borrow_mut()
            .retain(|_, value| !Rc::ptr_eq(value, &pending));
        // A close coordinator needs the outstanding handle even when cancelling
        // its future races with the method reply. It clears these after Close.
        if !self.cancel.is_cancelled() {
            self.active_requests.borrow_mut().remove(&expected);
            self.active_requests.borrow_mut().remove(&returned);
        }
        outcome
    }

    fn signal(&self, interface: &str, member: &str, path: &str, body: &glib::Variant) {
        if self.cancel.is_cancelled() {
            return;
        }
        if interface == REQUEST && member == "Response" {
            let response = body.get::<(u32, Properties)>().ok_or_else(|| {
                PortalError::from("The portal returned an invalid request response.")
            });
            let send = self
                .pending
                .borrow()
                .get(path)
                .and_then(|pending| pending.borrow_mut().take());
            if let Some(send) = send {
                let _ = send.send(response);
            } else if self.early.borrow().len() < 8 {
                self.early.borrow_mut().insert(path.into(), response);
            }
        } else if interface == SESSION && member == "Closed" {
            if self.handle.borrow().as_deref() == Some(path) {
                self.handle.borrow_mut().take();
                (self.callbacks.error)("The desktop closed Mluva's global shortcut session.");
            }
        } else if interface == SHORTCUTS
            && path == PATH
            && let Err(error) = self.shortcut_signal(member, body)
        {
            (self.callbacks.error)(&error.0);
        }
    }

    fn shortcut_signal(&self, member: &str, body: &glib::Variant) -> Result<()> {
        if ["Activated", "Deactivated"].contains(&member) {
            if body.n_children() != 4 {
                return Err("The portal returned an invalid shortcut activation.".into());
            }
            if !self.matches_session(&body.child_value(0)) {
                return Ok(());
            }
            let id = body.child_value(1);
            let Some(id) = id.str() else {
                return Ok(());
            };
            if member == "Deactivated" {
                self.pressed.borrow_mut().remove(id);
                return Ok(());
            }
            if ![self.recording_id.as_str(), CANCEL, REWRITE].contains(&id)
                || !self.pressed.borrow_mut().insert(id.into())
            {
                return Ok(());
            }
            if id == self.recording_id {
                (self.callbacks.toggle_recording)();
            } else if id == REWRITE {
                (self.callbacks.open_rewrite)();
            } else {
                (self.callbacks.cancel)();
            }
        } else if member == "ShortcutsChanged"
            && body.n_children() == 2
            && self.matches_session(&body.child_value(0))
        {
            self.report_bindings(&bound_shortcuts(&body.child_value(1))?);
        }
        Ok(())
    }

    fn matches_session(&self, supplied: &glib::Variant) -> bool {
        self.handle
            .borrow()
            .as_deref()
            .is_some_and(|owned| supplied.str() == Some(owned))
    }

    pub async fn close(&self) {
        self.cancel.cancel();
        let _serial = self.cleanup.lock().await;
        let Some(bus) = self.bus.borrow_mut().take() else {
            return;
        };
        let requests = std::mem::take(&mut *self.active_requests.borrow_mut());
        let handle = self.handle.borrow_mut().take();
        for (path, interface) in requests
            .into_iter()
            .map(|path| (path, REQUEST))
            .chain(handle.map(|path| (path, SESSION)))
        {
            // Only validated caller-owned paths reach this set.
            let message =
                gio::DBusMessage::new_method_call(Some(PORTAL), &path, Some(interface), "Close");
            let _ = bus
                .send_message_with_reply_future(&message, gio::DBusSendMessageFlags::NONE, 2000)
                .await;
        }
        self.pending.borrow_mut().clear();
        self.early.borrow_mut().clear();
        self.pressed.borrow_mut().clear();
        self.subscriptions.borrow_mut().clear();
        let _ = bus.close_future().await;
    }
}
fn token(prefix: &str) -> String {
    format!("{prefix}{}", glib::uuid_string_random().replace('-', ""))
}
fn sender(bus: &gio::DBusConnection) -> Result<String> {
    bus.unique_name()
        .as_deref()
        .and_then(|name| name.strip_prefix(':'))
        .map(|name| name.replace('.', "_"))
        .ok_or_else(|| "The session bus did not assign a unique caller name.".into())
}
