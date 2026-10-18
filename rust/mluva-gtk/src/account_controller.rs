//! Account UI and orchestration live beside capture, never inside the speech route.
use crate::{async_runtime::DesktopRuntime, conversation_view::ConversationWorkspace};
use adw::prelude::*;
use futures_util::{SinkExt, StreamExt};
use mluva_core::{
    account::{AccountLedger, AccountNote},
    conversation::ConversationStore,
};
use mluva_providers::account::{ACCOUNT_ORIGIN, AccountClient, AccountConfig, AccountLogin};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::{BTreeMap, BTreeSet},
    rc::Rc,
    time::Duration,
};
use tokio_tungstenite::tungstenite::Message;
use tokio_util::sync::CancellationToken;

type Refresh = Rc<dyn Fn()>;
pub struct AccountController {
    pub page: adw::PreferencesPage,
    pub status: gtk::Label,
    pub sign_in: gtk::Button,
    pub sign_out: gtk::Button,
    pub name: adw::EntryRow,
    pub sync_new: adw::SwitchRow,
    pub incoming: gtk::Box,
    pub incoming_text: gtk::Label,
    devices: adw::PreferencesGroup,
    device_rows: RefCell<Vec<adw::ActionRow>>,
    conflict_rows: RefCell<BTreeMap<String, adw::ExpanderRow>>,
    private_mode: Cell<bool>,
    conflicts: adw::PreferencesGroup,
    runtime: Rc<DesktopRuntime>,
    store: ConversationStore,
    workspace: Rc<ConversationWorkspace>,
    refresh: Refresh,

    client: RefCell<Option<AccountClient>>,
    ledger: RefCell<Option<AccountLedger>>,
    login: RefCell<Option<AccountLogin>>,
    config: RefCell<Option<AccountConfig>>,
    cancelled: RefCell<CancellationToken>,
    generation: Cell<u64>,
    syncing: Cell<bool>,
    known: RefCell<BTreeSet<String>>,
    selected: RefCell<BTreeSet<String>>,
    live_sender: RefCell<Option<tokio::sync::mpsc::Sender<Value>>>,
    live_session: RefCell<Option<(String, u64, String)>>,
    incoming_sessions: RefCell<BTreeMap<String, (u64, String, String)>>,
}
impl AccountController {
    pub fn new(
        runtime: Rc<DesktopRuntime>,
        store: ConversationStore,
        workspace: Rc<ConversationWorkspace>,
        refresh: Refresh,
    ) -> Rc<Self> {
        let page = adw::PreferencesPage::builder()
            .name("account")
            .title("Account & devices")
            .build();
        let group=adw::PreferencesGroup::builder().title("One account, your devices").description("Optional text sync. Native recognition keeps its selected provider. Local models and microphone audio stay on this PC.").build();
        let status = gtk::Label::builder()
            .label("Signed out · local recording is ready")
            .wrap(true)
            .xalign(0.0)
            .build();
        group.add(&status);
        let name = adw::EntryRow::builder()
            .title("This device’s name")
            .text("My Linux PC")
            .build();
        group.add(&name);
        let rename = gtk::Button::with_label("Save device name");
        group.add(&rename);
        let sign_in = gtk::Button::with_label("Sign in with browser");
        sign_in.add_css_class("suggested-action");
        group.add(&sign_in);
        let sign_out = gtk::Button::with_label("Sign out & disconnect this device");
        sign_out.set_visible(false);
        sign_out.set_tooltip_text(Some("Sync or export unfinished changes first. Downloaded account notes are removed from this PC; original local recordings stay."));
        group.add(&sign_out);
        let sync_new = adw::SwitchRow::builder()
            .title("Sync new recordings automatically")
            .subtitle("Shared conversations sync their saved edits. Existing private history stays local until you share it.")
            .build();
        group.add(&sync_new);
        let upload = gtk::Button::with_label("Share the open conversation…");
        group.add(&upload);
        let retry = gtk::Button::with_label("Reconnect & retry sync");
        group.add(&retry);
        page.add(&group);
        let devices = adw::PreferencesGroup::builder()
            .title("Send live words to")
            .description("Choose receivers. Incoming words never copy or paste automatically.")
            .build();
        page.add(&devices);
        let conflicts = adw::PreferencesGroup::builder()
            .title("Changes to review")
            .build();
        page.add(&conflicts);
        let incoming = gtk::Box::new(gtk::Orientation::Vertical, 6);
        incoming.set_margin_start(24);
        incoming.set_margin_end(24);
        incoming.set_margin_top(8);
        let incoming_text = gtk::Label::builder()
            .wrap(true)
            .selectable(true)
            .xalign(0.0)
            .build();
        incoming.append(&incoming_text);
        incoming.set_visible(false);
        let known = store
            .history
            .recent(i64::MAX)
            .unwrap_or_default()
            .into_iter()
            .map(|r| r.identifier)
            .collect();
        let owner = Rc::new(Self {
            page,
            status,
            sign_in,
            sign_out,
            name,
            sync_new,
            incoming,
            incoming_text,
            devices,
            device_rows: RefCell::new(Vec::new()),
            conflict_rows: RefCell::new(BTreeMap::new()),
            private_mode: Cell::new(false),
            conflicts,
            runtime,
            store,
            workspace,
            refresh,
            client: RefCell::new(None),
            ledger: RefCell::new(None),
            login: RefCell::new(None),
            config: RefCell::new(None),
            cancelled: RefCell::new(CancellationToken::new()),
            generation: Cell::new(0),
            syncing: Cell::new(false),
            known: RefCell::new(known),
            selected: RefCell::new(BTreeSet::new()),
            live_sender: RefCell::new(None),
            live_session: RefCell::new(None),
            incoming_sessions: RefCell::new(BTreeMap::new()),
        });
        let weak = Rc::downgrade(&owner);
        owner.sign_in.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.begin_login();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.sign_out.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.disconnect();
            }
        });
        let weak = Rc::downgrade(&owner);
        retry.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.sync();
            }
        });
        let weak = Rc::downgrade(&owner);
        upload.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.share_current();
            }
        });
        let weak = Rc::downgrade(&owner);
        rename.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                o.rename_device();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.sync_new.connect_active_notify(move |_| {
            if let Some(o) = weak.upgrade() {
                o.save_preferences();
            }
        });
        owner
    }
    fn cache_path(&self) -> std::path::PathBuf {
        self.store
            .history
            .database
            .path
            .with_file_name("account-host.json")
    }
    fn save_preferences(&self) {
        if let Some(ledger) = self.ledger.borrow().as_ref() {
            let _ = ledger.save_preferences(
                &json!({"syncNew":self.sync_new.is_active(), "selected":*self.selected.borrow()}),
            );
        }
    }
    fn origin() -> String {
        std::env::var("MLUVA_ACCOUNT_ORIGIN").unwrap_or_else(|_| ACCOUNT_ORIGIN.into())
    }
    pub fn restore(self: &Rc<Self>) {
        if self.private_mode.get() {
            return;
        }
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        // Public configuration is cached separately from keyring credentials. Removing this
        // opt-in file on sign-out also prevents a locked keyring from restoring an old session.
        let Some(config) = std::fs::read(self.cache_path())
            .ok()
            .and_then(|v| serde_json::from_slice::<AccountConfig>(&v).ok())
        else {
            return;
        };
        if config.validate(&Self::origin()).is_err() {
            return;
        }
        self.runtime.spawn(async move {
            let result = AccountClient::restore(config).await;
            if let Some(o) = weak.upgrade().filter(|o| o.generation.get() == generation)
                && let Ok(Some(client)) = result
            {
                o.activate(client, false).await;
            }
        });
    }
    pub fn begin_login(self: &Rc<Self>) {
        self.status
            .set_label("Opening secure sign-in in your browser…");
        self.sign_in.set_sensitive(false);
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        self.runtime.spawn(async move {
            let result = AccountConfig::load(&Self::origin())
                .await
                .and_then(AccountLogin::new);
            if let Some(o) = weak.upgrade().filter(|o| o.generation.get() == generation) {
                o.sign_in.set_sensitive(true);
                match result {
                    Ok(login) => {
                        let url = login.url.clone();
                        o.login.replace(Some(login));
                        if gio::AppInfo::launch_default_for_uri(&url, gio::AppLaunchContext::NONE)
                            .is_err()
                        {
                            o.status.set_label(
                                "Could not open the sign-in browser. Check your default browser.",
                            );
                        } else {
                            o.status.set_label(
                                "Finish signing in in your browser, then allow it to open Mluva.",
                            );
                        }
                    }
                    Err(e) => o.status.set_label(&e.to_string()),
                }
            }
        });
    }
    pub fn callback(self: &Rc<Self>, uri: &str) {
        let Some(login) = self.login.borrow_mut().take() else {
            self.status
                .set_label("Start sign-in from Mluva before returning from the browser.");
            return;
        };
        let uri = uri.to_owned();
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        self.runtime.spawn(async move {
            let result = login.finish(&uri).await;
            if let Some(o) = weak.upgrade().filter(|o| o.generation.get() == generation) {
                match result {
                    Ok(client) => o.activate(client, true).await,
                    Err(e) => o.status.set_label(&e.to_string()),
                }
            }
        });
    }
    pub async fn activate(self: &Rc<Self>, client: AccountClient, register: bool) {
        if self.private_mode.get() {
            return;
        }
        let generation = self.generation.get();
        let (owner, id) = client.identity().await;
        if register
            && let Err(e) = client
                .api(
                    "POST",
                    "/devices",
                    json!({"id":id,"name":self.name.text().as_str(),"kind":"laptop"}),
                )
                .await
        {
            self.status.set_label(&e.to_string());
            return;
        }
        if self.generation.get() != generation || self.private_mode.get() {
            return;
        }
        let ledger = match AccountLedger::new(
            self.store.clone(),
            format!("{}|{owner}", client.config.origin),
        ) {
            Ok(l) => l,
            Err(e) => {
                self.status.set_label(&e.to_string());
                return;
            }
        };
        self.cancelled.borrow().cancel();
        self.cancelled.replace(CancellationToken::new());
        self.known.replace(
            self.store
                .history
                .recent(i64::MAX)
                .unwrap_or_default()
                .into_iter()
                .map(|e| e.identifier)
                .collect(),
        );
        let preferences = ledger.preferences().unwrap_or_default();
        self.ledger.replace(None);
        self.sync_new
            .set_active(preferences["syncNew"].as_bool().unwrap_or(false));
        self.selected
            .replace(serde_json::from_value(preferences["selected"].clone()).unwrap_or_default());
        self.ledger.replace(Some(ledger));
        if let Ok(config) = serde_json::to_vec(&client.config) {
            let _ = mluva_core::private_files::atomic_write_private(&self.cache_path(), &config);
        }
        self.config.replace(Some(client.config.clone()));
        self.client.replace(Some(client.clone()));
        self.sign_in.set_visible(false);
        self.sign_out.set_visible(true);
        self.status.set_label("Connected · text sync ready");
        self.start_wire(client.clone());
        self.sync();
        if register {
            let cancel = self.cancelled.borrow().clone();
            let remembered = tokio::select! { _ = cancel.cancelled() => return, result = client.remember() => result };
            if let Err(e) = remembered {
                self.status.set_label(&e.to_string());
            }
        }
        let weak = Rc::downgrade(self);
        let cancel = self.cancelled.borrow().clone();
        self.runtime.spawn(async move {loop {tokio::select!{_ = cancel.cancelled()=>break,_=tokio::time::sleep(Duration::from_secs(3))=>{if let Some(o)=weak.upgrade(){o.sync();}else{break;}}}}});
    }
    fn share_current(self: &Rc<Self>) {
        if self.private_mode.get() {
            self.status
                .set_label("Account sharing is paused in Incognito.");
            return;
        }
        let Some(entry) = self.workspace.entry() else {
            self.status
                .set_label("Open a saved conversation to share it.");
            return;
        };
        let Some(ledger) = self.ledger.borrow().clone() else {
            self.status
                .set_label("Sign in before sharing a conversation.");
            return;
        };
        let dialog=adw::MessageDialog::builder().heading("Share this conversation?").body("Its original words, saved edits and completed rewrites will be stored in your account. Retained audio and screenshots stay on this PC.").build();
        if let Some(window) = self
            .page
            .root()
            .and_then(|r| r.downcast::<gtk::Window>().ok())
        {
            dialog.set_transient_for(Some(&window));
        }
        dialog.add_responses(&[("cancel", "Keep local"), ("share", "Share text")]);
        dialog.set_close_response("cancel");
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        dialog.connect_response(None, move |_, response| {
            if response == "share"
                && let Some(o) = weak
                    .upgrade()
                    .filter(|o| o.generation.get() == generation && !o.private_mode.get())
            {
                if let Err(e) = ledger.track(&entry.identifier) {
                    o.status.set_label(&e.to_string());
                } else {
                    o.sync();
                }
            }
        });
        dialog.present();
    }
    pub fn sync(self: &Rc<Self>) {
        if self.private_mode.get() {
            return;
        }
        if self.syncing.replace(true) {
            return;
        }
        let Some(client) = self.client.borrow().clone() else {
            self.syncing.set(false);
            return;
        };
        let Some(ledger) = self.ledger.borrow().clone() else {
            self.syncing.set(false);
            return;
        };
        let current = self.store.history.recent(i64::MAX).unwrap_or_default();
        for entry in &current {
            if entry.raw_text.trim().is_empty() && entry.delivered_text.trim().is_empty() {
                continue;
            }
            if self
                .store
                .history
                .recording_conversation(&entry.identifier)
                .is_ok_and(|parent| parent.identifier != entry.identifier)
            {
                continue;
            }
            if self.sync_new.is_active() && !self.known.borrow().contains(&entry.identifier) {
                let _ = ledger.track(&entry.identifier);
            }
            self.known.borrow_mut().insert(entry.identifier.clone());
        }
        let weak = Rc::downgrade(self);
        let cancel = self.cancelled.borrow().clone();
        self.runtime.spawn(async move {
            let operation = async {
                let mut rejected = None;
                for id in ledger.ids().map_err(|e| e.to_string())? {
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    if ledger
                        .store
                        .history
                        .recording_conversation(&id)
                        .is_ok_and(|parent| parent.identifier != id)
                    {
                        continue;
                    }
                    if ledger.store.history.find(&id).is_err() {
                        client
                            .api("DELETE", &format!("/history/{id}"), Value::Null)
                            .await
                            .map_err(|e| e.to_string())?;
                        ledger.delete(&id).map_err(|e| e.to_string())?;
                        continue;
                    }
                    if let Some(edit) = ledger.pending(&id).map_err(|e| e.to_string())? {
                        let mut body =
                            serde_json::to_value(&edit.note).map_err(|e| e.to_string())?;
                        body["operationId"] = json!(edit.operation_id);
                        body["baseRevision"] = json!(edit.base_revision);
                        let merges: Vec<Value> = edit
                            .merged_sources
                            .iter()
                            .map(|(id, revision)| json!({"id":id,"baseRevision":revision}))
                            .collect();
                        body["mergedSources"] = json!(merges);
                        let result = if edit.base_revision == 0 {
                            client.api("POST", "/history", body).await
                        } else {
                            client.api("PUT", &format!("/history/{id}"), body).await
                        };
                        if cancel.is_cancelled() {
                            return Ok(());
                        }
                        match result {
                            Ok(row) => {
                                let note: AccountNote =
                                    serde_json::from_value(row).map_err(|e| e.to_string())?;
                                ledger.acknowledge(&note).map_err(|e| e.to_string())?;
                                for source in edit.merged_sources.keys() {
                                    ledger.delete(source).map_err(|e| e.to_string())?;
                                }
                            }
                            Err(e) if e.status == 410 => {
                                ledger.delete(&id).map_err(|e| e.to_string())?;
                                if let Some(o) = weak.upgrade() {
                                    (o.refresh)();
                                }
                            }
                            Err(e) if e.status == 409 => {
                                if let Ok(row) = client
                                    .api("GET", &format!("/history/{id}"), Value::Null)
                                    .await
                                    && let (Some(o), Ok(note)) =
                                        (weak.upgrade(), serde_json::from_value(row))
                                {
                                    o.conflict(note);
                                }
                            }
                            Err(e) if e.status == 400 || e.status == 413 => {
                                ledger
                                    .discard_rejected_edit(&id)
                                    .map_err(|e| e.to_string())?;
                                rejected = Some(e.to_string());
                            }
                            Err(e) => return Err(e.to_string()),
                        }
                    }
                }
                let mut cursor = String::new();
                let mut seen = BTreeSet::new();
                loop {
                    let page = client
                        .api("GET", &format!("/history{cursor}"), Value::Null)
                        .await
                        .map_err(|e| e.to_string())?;
                    if cancel.is_cancelled() {
                        return Ok(());
                    }
                    for row in page["entries"]
                        .as_array()
                        .ok_or("Invalid account history")?
                    {
                        let note: AccountNote =
                            serde_json::from_value(row.clone()).map_err(|e| e.to_string())?;
                        seen.insert(note.id.clone());
                        if let Some(o) = weak.upgrade() {
                            if o.workspace.has_unsaved_edits(&note.id)
                                || note
                                    .segments
                                    .iter()
                                    .any(|s| o.workspace.has_unsaved_edits(&s.id))
                            {
                                continue;
                            }
                            match ledger.receive(&note) {
                                Ok(true) => {
                                    o.known.borrow_mut().insert(note.id.clone());
                                    o.refresh_received(&note.id);
                                }
                                Ok(false) => {}
                                Err(_) => o.conflict(note),
                            }
                        }
                    }
                    if let Some(next) = page["cursor"].as_str() {
                        cursor = format!("?cursor={next}");
                    } else {
                        break;
                    }
                }
                for id in ledger.ids().map_err(|e| e.to_string())? {
                    if !seen.contains(&id)
                        && ledger.baseline(&id).map_err(|e| e.to_string())?.is_some()
                        && let Err(e) = client
                            .api("GET", &format!("/history/{id}"), Value::Null)
                            .await
                        && e.status == 410
                        && !cancel.is_cancelled()
                    {
                        ledger.delete(&id).map_err(|e| e.to_string())?;
                        if let Some(o) = weak.upgrade() {
                            (o.refresh)();
                        }
                    }
                }
                rejected.map_or(Ok::<(), String>(()), Err)
            };
            let result = tokio::select! {_=cancel.cancelled()=>Ok(()),result=operation=>result};
            if let Some(o) = weak.upgrade() {
                o.syncing.set(false);
                if !cancel.is_cancelled()
                    && let Err(e) = result
                {
                    o.status.set_label(&e);
                }
            }
        });
    }
    fn conflict(self: &Rc<Self>, remote: AccountNote) {
        if self.conflict_rows.borrow().contains_key(&remote.id) {
            return;
        }
        let row = adw::ExpanderRow::builder()
            .title(format!("{} · changed on another device", remote.title))
            .subtitle("Your saved edit is kept. Compare before choosing a version.")
            .build();
        let text = gtk::Label::builder()
            .label(format!(
                "Your saved version\n{}\n\nAccount version\n{}",
                self.store
                    .history
                    .find(&remote.id)
                    .ok()
                    .and_then(|entry| self.store.source_text(&entry, false).ok())
                    .unwrap_or_default(),
                remote.text
            ))
            .wrap(true)
            .selectable(true)
            .xalign(0.0)
            .build();
        row.add_row(&text);
        let use_remote = gtk::Button::with_label("Use the account version");
        row.add_row(&use_remote);
        let keep_local = gtk::Button::with_label("Keep my saved version");
        row.add_row(&keep_local);
        // Replace one conflict row per note, rather than accumulating retries.
        row.set_widget_name(&remote.id);
        let weak = Rc::downgrade(self);
        let note = remote.clone();
        let target = row.clone();
        use_remote.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade()
                && let Some(l) = o.ledger.borrow().as_ref()
                && l.apply(&note).is_ok()
            {
                o.conflicts.remove(&target);
                o.conflict_rows
                    .borrow_mut()
                    .remove(target.widget_name().as_str());
                (o.refresh)();
            }
        });
        let weak = Rc::downgrade(self);
        let target = row.clone();
        keep_local.connect_clicked(move |_| {
            if let Some(o) = weak.upgrade() {
                if let Some(l) = o.ledger.borrow().as_ref() {
                    let _ = l.acknowledge(&remote);
                }
                o.conflicts.remove(&target);
                o.conflict_rows
                    .borrow_mut()
                    .remove(target.widget_name().as_str());
                o.sync();
            }
        });
        self.conflict_rows
            .borrow_mut()
            .insert(row.widget_name().into(), row.clone());
        self.conflicts.add(&row);
        self.status
            .set_label("Concurrent changes need review in Account settings.");
    }
    fn start_wire(self: &Rc<Self>, client: AccountClient) {
        let (send, mut receive) = tokio::sync::mpsc::channel::<Value>(64);
        self.live_sender.replace(Some(send));
        let weak = Rc::downgrade(self);
        let cancel = self.cancelled.borrow().clone();
        self.runtime.spawn(async move {
            let mut pending: Option<Value> = None;
            loop {
                let connection = tokio::select! { _ = cancel.cancelled() => break, r = client.connect() => r };
                match connection {
                    Ok(mut socket) => {
                        let _ = socket.send(Message::Text(json!({"action":"hello"}).to_string().into())).await;
                        if let Some(o) = weak.upgrade() { o.status.set_label("Connected · text sync ready"); o.load_devices(); o.sync(); }
                        if let Some(message) = &pending && socket.send(Message::Text(message.to_string().into())).await.is_err() { continue; }
                        let mut heartbeat = tokio::time::interval(Duration::from_secs(10));
                        let mut sent_at = std::time::Instant::now();
                        loop {
                            tokio::select! {
                                _ = cancel.cancelled() => { let _ = socket.close(None).await; return; },
                                _ = heartbeat.tick() => {
                                    if pending.is_some() && sent_at.elapsed() > Duration::from_secs(12) { break; }
                                    if socket.send(Message::Text(json!({"action":"ping"}).to_string().into())).await.is_err() { break; }
                                },
                                message = receive.recv(), if pending.is_none() => {
                                    let Some(message) = message else { return; };
                                    pending = Some(message); sent_at = std::time::Instant::now();
                                    if socket.send(Message::Text(pending.as_ref().unwrap().to_string().into())).await.is_err() { break; }
                                },
                                message = socket.next() => { match message {
                                    Some(Ok(Message::Text(data))) => {
                                        if let Ok(event) = serde_json::from_str::<Value>(&data) {
                                            if event["type"] == "ack" && pending.as_ref().is_some_and(|p| p["sessionId"] == event["sessionId"] && p["sequence"] == event["sequence"]) { pending = None; }
                                            if event["type"] == "error" {
                                                if let Some(o) = weak.upgrade() { o.status.set_label("Live words need reconnecting. The complete note will sync from History."); }
                                                if event["status"].as_u64().is_some_and(|s| s == 401 || s == 403) { if let Some(o)=weak.upgrade(){o.disconnect();} return; }
                                                pending = None;
                                            }
                                            if let Some(o) = weak.upgrade() { o.event(event); }
                                        }
                                    },
                                    Some(Ok(Message::Close(_))) | None | Some(Err(_)) => break,
                                    _ => {}
                                }}
                            }
                        }
                    },
                    Err(e) => { if let Some(o) = weak.upgrade() { if e.status == 401 || e.status == 403 { o.disconnect(); return; } o.status.set_label(&e.to_string()); } }
                }
                if let Some(o) = weak.upgrade() { o.status.set_label("Offline · native recording continues. Saved text will retry."); }
                tokio::select! { _ = cancel.cancelled() => break, _ = tokio::time::sleep(Duration::from_secs(3)) => {} }
            }
        });
    }
    fn load_devices(self: &Rc<Self>) {
        let Some(client) = self.client.borrow().clone() else {
            return;
        };
        let weak = Rc::downgrade(self);
        let generation = self.generation.get();
        self.runtime.spawn(async move {
            let result = client.api("GET", "/devices", Value::Null).await;
            let (_, current) = client.identity().await;
            if let Some(o) = weak.upgrade().filter(|o| o.generation.get() == generation)
                && let Ok(result) = result
                && let Some(rows) = result["devices"].as_array()
            {
                // PreferencesGroup owns rows; retain explicit children for refresh.
                o.render_devices(rows, &current);
            }
        });
    }
    fn render_devices(self: &Rc<Self>, rows: &[Value], current: &str) {
        for row in self.device_rows.borrow_mut().drain(..) {
            self.devices.remove(&row);
        }
        let valid: BTreeSet<String> = rows
            .iter()
            .filter_map(|r| r["id"].as_str().map(str::to_owned))
            .collect();
        self.selected.borrow_mut().retain(|id| valid.contains(id));
        for device in rows {
            let Some(id) = device["id"].as_str() else {
                continue;
            };
            if id == current {
                self.name
                    .set_text(device["name"].as_str().unwrap_or("My Linux PC"));
                continue;
            }
            let row = adw::ActionRow::builder()
                .title(device["name"].as_str().unwrap_or("Device"))
                .subtitle(if device["online"].as_bool() == Some(true) {
                    "Connected"
                } else {
                    "Offline"
                })
                .build();
            let choice = gtk::CheckButton::builder()
                .active(self.selected.borrow().contains(id))
                .valign(gtk::Align::Center)
                .build();
            choice.set_tooltip_text(Some("Send live words to this device"));
            row.add_prefix(&choice);
            let owned_id = id.to_owned();
            let weak = Rc::downgrade(self);
            choice.connect_toggled(move |button| {
                if let Some(o) = weak.upgrade() {
                    if button.is_active() {
                        o.selected.borrow_mut().insert(owned_id.clone());
                    } else {
                        o.selected.borrow_mut().remove(&owned_id);
                    }
                    o.save_preferences();
                }
            });
            let remove = gtk::Button::builder()
                .label("Remove")
                .valign(gtk::Align::Center)
                .build();
            row.add_suffix(&remove);
            let owned_id = id.to_owned();
            let weak = Rc::downgrade(self);
            remove.connect_clicked(move |_| {
                if let Some(o) = weak.upgrade() {
                    o.remove_device(&owned_id);
                }
            });
            self.devices.add(&row);
            self.device_rows.borrow_mut().push(row);
        }
    }
    fn rename_device(self: &Rc<Self>) {
        let Some(client) = self.client.borrow().clone() else {
            return;
        };
        let name = self.name.text().to_string();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let (_, id) = client.identity().await;
            let result = client
                .api(
                    "POST",
                    "/devices",
                    json!({"id":id,"name":name,"kind":"laptop"}),
                )
                .await;
            if let Some(o) = weak.upgrade() {
                match result {
                    Ok(_) => o.status.set_label("Device name saved."),
                    Err(e) => o.status.set_label(&e.to_string()),
                }
            }
        });
    }
    fn remove_device(self: &Rc<Self>, id: &str) {
        let Some(client) = self.client.borrow().clone() else {
            return;
        };
        let id = id.to_owned();
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let result = client
                .api("DELETE", &format!("/devices/{id}"), Value::Null)
                .await;
            if let Some(o) = weak.upgrade() {
                match result {
                    Ok(_) => o.load_devices(),
                    Err(e) => o.status.set_label(&e.to_string()),
                }
            }
        });
    }
    pub fn privacy(self: &Rc<Self>, private: bool) {
        if self.private_mode.replace(private) == private {
            return;
        }
        self.sync_new.set_sensitive(!private);
        self.sign_in.set_sensitive(!private);
        if private {
            self.generation.set(self.generation.get() + 1);
            self.login.borrow_mut().take();
            self.cancelled.borrow().cancel();
            self.incoming.set_visible(false);
            self.live_sender.borrow_mut().take();
            self.live_session.borrow_mut().take();
            self.status
                .set_label("Incognito · account sync and incoming history are paused");
        } else if let Some(client) = self.client.borrow().clone() {
            let weak = Rc::downgrade(self);
            self.runtime.spawn(async move {
                if let Some(o) = weak.upgrade() {
                    o.activate(client, false).await;
                }
            });
        } else {
            self.status
                .set_label("Signed out · local recording is ready");
            self.restore();
        }
    }
    fn event(self: &Rc<Self>, event: Value) {
        if self.private_mode.get() {
            return;
        }
        match event["type"].as_str() {
            Some("stream") => {
                let id = event["sessionId"].as_str().unwrap_or("").to_owned();
                let sequence = event["sequence"].as_u64().unwrap_or(0);
                let mut sessions = self.incoming_sessions.borrow_mut();
                let s = sessions
                    .entry(id)
                    .or_insert((u64::MAX, String::new(), String::new()));
                if s.0 != u64::MAX && sequence <= s.0 {
                    return;
                }
                s.0 = sequence;
                match event["phase"].as_str() {
                    Some("partial") => s.2 = event["text"].as_str().unwrap_or("").into(),
                    Some("segment") => {
                        if !s.1.is_empty() {
                            s.1.push('\n');
                        }
                        s.1.push_str(event["text"].as_str().unwrap_or(""));
                        s.2.clear();
                    }
                    Some("stopped") => s.2.clear(),
                    _ => {}
                }
                self.incoming_text.set_label(&format!(
                    "{} · {}\n{}{}",
                    event["deviceName"].as_str().unwrap_or("Your device"),
                    if event["phase"] == "stopped" {
                        "Finished · see History after saving"
                    } else {
                        "Live words"
                    },
                    s.1,
                    s.2
                ));
                self.incoming.set_visible(true);
                while sessions.len() > 6 {
                    let first = sessions.keys().next().cloned().unwrap();
                    sessions.remove(&first);
                }
            }
            Some("history.changed") => {
                if event["deleted"].as_bool() == Some(true)
                    && event["mergedInto"].is_null()
                    && let (Some(l), Some(id)) =
                        (self.ledger.borrow().as_ref(), event["id"].as_str())
                {
                    let _ = l.delete(id);

                    (self.refresh)();
                }
                self.sync();
            }
            Some("devices.changed") => self.load_devices(),
            Some("revoked") => self.disconnect(),
            _ => {}
        }
    }
    pub fn live(&self, id: &str, preview: &str, finished: Option<&str>) {
        if self.private_mode.get()
            || !self.sync_new.is_active()
            || self.selected.borrow().is_empty()
        {
            return;
        }
        let sender = self.live_sender.borrow();
        let Some(sender) = sender.as_ref() else {
            return;
        };
        let mut state = self.live_session.borrow_mut();
        if state.as_ref().is_none_or(|s| s.0 != id) {
            *state = Some((id.into(), 0, String::new()));
        }
        let s = state.as_mut().unwrap();
        let send = |phase: &str, text: &str, sequence: u64| {
            sender.try_send(json!({"action":"stream","phase":phase,"sessionId":id,"sequence":sequence,"text":text,"targets":*self.selected.borrow()})).is_ok()
        };
        if s.1 == 0 {
            if !send("started", "", 0) {
                return;
            }
            s.1 += 1;
        }
        if let Some(text) = finished {
            // Native previews are speculative; only final recognition is sent as settled segments.
            let mut part = String::new();
            for c in text.chars() {
                if part.len() + c.len_utf8() > 15000 {
                    if !send("segment", &part, s.1) {
                        self.status.set_label(
                            "Live receiver is behind. The complete note will sync through History.",
                        );
                        return;
                    }
                    s.1 += 1;
                    part.clear();
                }
                part.push(c);
            }
            if !part.is_empty() {
                if !send("segment", &part, s.1) {
                    return;
                }
                s.1 += 1;
            }
            if send("stopped", "", s.1) {
                *state = None;
            }
        } else if preview != s.2 {
            let tail = preview
                .chars()
                .rev()
                .take(3500)
                .collect::<String>()
                .chars()
                .rev()
                .collect::<String>();
            if send("partial", &tail, s.1) {
                s.1 += 1;
                s.2 = preview.into();
            }
        }
    }
    pub fn cancel_live(&self, id: &str) {
        let mut state = self.live_session.borrow_mut();
        if let (Some((current, sequence, _)), Some(sender)) =
            (state.as_ref(), self.live_sender.borrow().as_ref())
            && current == id
        {
            let _ = sender.try_send(json!({"action":"stream","phase":"stopped","sessionId":id,"sequence":sequence,"text":"","targets":*self.selected.borrow()}));
        }
        state.take();
    }
    pub fn disconnect(self: &Rc<Self>) {
        self.generation.set(self.generation.get() + 1);
        self.cancelled.borrow().cancel();
        self.login.borrow_mut().take();
        self.live_sender.borrow_mut().take();
        self.live_session.borrow_mut().take();
        self.incoming_sessions.borrow_mut().clear();
        self.incoming.set_visible(false);
        let _ = std::fs::remove_file(self.cache_path());
        for row in self.device_rows.borrow_mut().drain(..) {
            self.devices.remove(&row);
        }
        for (_, row) in self.conflict_rows.borrow_mut().split_off("") {
            self.conflicts.remove(&row);
        }
        self.selected.borrow_mut().clear();
        let old_ledger = self.ledger.borrow_mut().take();
        let cleaned = if let Some(ledger) = old_ledger {
            if ledger.sign_out().is_err() {
                self.ledger.replace(Some(ledger));
                false
            } else {
                true
            }
        } else {
            true
        };
        self.sign_in.set_visible(true);
        self.sign_in
            .set_sensitive(cleaned && !self.private_mode.get());
        self.sign_out.set_visible(!cleaned);
        self.sync_new.set_active(false);
        self.status
            .set_label("Signed out · local recordings and models are preserved");
        if !cleaned {
            self.status.set_label("Sync stopped. Downloaded account notes could not be cleared. Retry Sign out before connecting another account.");
        }
        (self.refresh)();
        if let Some(client) = self.client.borrow_mut().take() {
            let weak = Rc::downgrade(self);
            let generation = self.generation.get();
            self.runtime.spawn(async move{if client.sign_out().await.is_err()&& let Some(o)=weak.upgrade().filter(|o|o.generation.get()==generation && o.client.borrow().is_none()){o.status.set_label("Signed out here. Offline device revocation could not reach the account; remove this device from another signed-in device.");}});
        }
    }
    fn refresh_received(&self, id: &str) {
        let _ = self.workspace.refresh_history();
        if self.workspace.entry().is_some_and(|e| e.identifier == id)
            && !self.workspace.has_unsaved_edits(id)
            && let Ok(entry) = self.store.history.find(id)
        {
            let replies = self.store.replies(id).unwrap_or_default();
            let _ = self
                .workspace
                .show_conversation(Some(entry), &replies, false);
        }
    }
    pub fn shutdown(&self) {
        self.cancelled.borrow().cancel();
        self.login.borrow_mut().take();
    }
}
