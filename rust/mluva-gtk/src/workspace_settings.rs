//! Recorder appearance and history display settings.
use crate::{
    appearance_settings::AppearanceSettings,
    settings_view::{SaveSettings, proposed_config},
};
use adw::prelude::*;
use mluva_core::config::AppConfig;
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
    time::Duration,
};

enum Field {
    Switch(adw::SwitchRow),
    Choice(adw::ComboRow, Vec<String>),
}
impl Field {
    fn read(&self) -> Value {
        match self {
            Self::Switch(row) => Value::Bool(row.is_active()),
            Self::Choice(row, values) => Value::String(values[row.selected() as usize].clone()),
        }
    }
    fn write(&self, value: &Value) {
        match self {
            Self::Switch(row) => row.set_active(value.as_bool().unwrap()),
            Self::Choice(row, values) => row.set_selected(
                values
                    .iter()
                    .position(|v| Some(v.as_str()) == value.as_str())
                    .unwrap() as u32,
            ),
        }
    }
}
pub struct WorkspaceSettings {
    pub widget: adw::PreferencesPage,
    pub appearance: Rc<AppearanceSettings>,
    pub status: gtk::Label,
    syncing: Cell<bool>,
    pending: Cell<bool>,
    timer: RefCell<Option<glib::SourceId>>,
    config: RefCell<AppConfig>,
    fields: BTreeMap<String, Field>,
    save: SaveSettings,
}
impl WorkspaceSettings {
    pub fn new(config: &AppConfig, save: SaveSettings) -> Rc<Self> {
        let widget = adw::PreferencesPage::builder()
            .name("workspace")
            .title("Workspace")
            .icon_name("preferences-system-symbolic")
            .build();
        let mut fields = BTreeMap::new();
        let document = serde_json::to_value(config).unwrap();
        let target: Rc<RefCell<Weak<Self>>> = Rc::new(RefCell::new(Weak::new()));
        let weak = target.clone();
        let appearance = AppearanceSettings::new(
            config,
            Rc::new(move || {
                if let Some(owner) = weak.borrow().upgrade().filter(|o| !o.syncing.get()) {
                    owner.schedule();
                }
            }),
        );
        widget.add(&appearance.widget);
        let group = adw::PreferencesGroup::builder().title("Appearance").build();
        Self::switch(
            &group,
            &mut fields,
            &document,
            "history_sidebar_visible",
            "Show history sidebar by default",
        );
        Self::choice(
            &group,
            &mut fields,
            &document,
            "time_format",
            "History time format",
            &[("24h", "24-hour · 14:30"), ("12h", "12-hour · 2:30 PM")],
        );
        widget.add(&group);
        let actions = adw::PreferencesGroup::new();
        let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
        actions.add(&status);
        widget.add(&actions);
        let owner = Rc::new(Self {
            widget,
            appearance,
            status,
            syncing: Cell::new(false),
            pending: Cell::new(false),
            timer: RefCell::new(None),
            config: RefCell::new(config.clone()),
            fields,
            save,
        });
        target.replace(Rc::downgrade(&owner));
        for field in owner.fields.values() {
            let weak = Rc::downgrade(&owner);
            match field {
                Field::Switch(row) => {
                    row.connect_active_notify(move |_| {
                        if let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) {
                            owner.pending.set(true);
                            owner.apply();
                        }
                    });
                }
                Field::Choice(row, _) => {
                    row.connect_selected_notify(move |_| {
                        if let Some(owner) = weak.upgrade().filter(|o| !o.syncing.get()) {
                            owner.pending.set(true);
                            owner.apply();
                        }
                    });
                }
            }
        }
        owner
    }
    fn switch(
        group: &adw::PreferencesGroup,
        fields: &mut BTreeMap<String, Field>,
        document: &Value,
        name: &str,
        title: &str,
    ) {
        let row = adw::SwitchRow::builder()
            .title(title)
            .active(document[name].as_bool().unwrap())
            .build();
        group.add(&row);
        fields.insert(name.into(), Field::Switch(row));
    }

    fn choice(
        group: &adw::PreferencesGroup,
        fields: &mut BTreeMap<String, Field>,
        document: &Value,
        name: &str,
        title: &str,
        choices: &[(&str, &str)],
    ) {
        let labels: Vec<_> = choices.iter().map(|(_, label)| *label).collect();
        let row = adw::ComboRow::builder()
            .title(title)
            .model(&gtk::StringList::new(&labels))
            .build();
        row.set_selected(
            choices
                .iter()
                .position(|(value, _)| Some(*value) == document[name].as_str())
                .unwrap() as u32,
        );
        group.add(&row);
        fields.insert(
            name.into(),
            Field::Choice(
                row,
                choices.iter().map(|(value, _)| (*value).into()).collect(),
            ),
        );
    }
    pub fn refresh_config(&self, config: &AppConfig) {
        if self.pending.get() {
            return;
        }
        self.syncing.set(true);
        self.config.replace(config.clone());
        self.appearance.refresh_config(config);
        let document = serde_json::to_value(config).unwrap();
        for (name, field) in &self.fields {
            field.write(&document[name]);
        }
        self.syncing.set(false);
    }
    pub fn values(&self) -> Map<String, Value> {
        let mut changes: Map<_, _> = self
            .fields
            .iter()
            .map(|(name, field)| (name.clone(), field.read()))
            .collect();
        changes.extend(self.appearance.values());
        changes
    }
    fn schedule(self: &Rc<Self>) {
        self.pending.set(true);
        if let Some(timer) = self.timer.borrow_mut().take() {
            timer.remove();
        }
        let weak = Rc::downgrade(self);
        self.timer.replace(Some(glib::timeout_add_local_once(
            Duration::from_millis(300),
            move || {
                if let Some(owner) = weak.upgrade() {
                    owner.timer.borrow_mut().take();
                    owner.apply();
                }
            },
        )));
    }
    pub fn apply(&self) {
        if !self.pending.get() {
            return;
        }
        let baseline = serde_json::to_value(&*self.config.borrow()).expect("serializable config");
        let changes = self
            .values()
            .into_iter()
            .filter(|(key, value)| baseline[key] != *value)
            .collect();
        let proposed = proposed_config(&self.config.borrow(), &changes);
        match proposed {
            Err(error) => self.status.set_label(&error.to_string()),
            Ok(proposed) => {
                self.status.set_label(if (self.save)(&changes) {
                    self.pending.set(false);
                    self.config.replace(proposed);
                    "Saved automatically"
                } else {
                    "Could not apply settings. Stop active work, check the provider and try again."
                });
            }
        }
    }
}
