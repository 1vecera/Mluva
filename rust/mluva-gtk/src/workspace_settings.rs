//! Recorder appearance and history display settings.
use crate::{
    appearance_settings::AppearanceSettings,
    settings_view::{SaveSettings, proposed_config},
};
use adw::prelude::*;
use mluva_core::config::AppConfig;
use serde_json::{Map, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

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
    pub apply_button: gtk::Button,
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
        let appearance = AppearanceSettings::new(config, Rc::new(|| {}));
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
        let row = adw::ActionRow::builder()
            .title("Save settings")
            .subtitle("Changes apply to the next request or recording.")
            .build();
        let apply_button = gtk::Button::builder()
            .label("Apply")
            .valign(gtk::Align::Center)
            .build();
        apply_button.add_css_class("suggested-action");
        row.add_suffix(&apply_button);
        actions.add(&row);
        let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
        actions.add(&status);
        widget.add(&actions);
        let owner = Rc::new(Self {
            widget,
            appearance,
            status,
            apply_button,
            config: RefCell::new(config.clone()),
            fields,
            save,
        });
        let weak = Rc::downgrade(&owner);
        owner.apply_button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.apply();
            }
        });
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
        self.config.replace(config.clone());
        self.appearance.refresh_config(config);
        let document = serde_json::to_value(config).unwrap();
        for (name, field) in &self.fields {
            field.write(&document[name]);
        }
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
    pub fn apply(&self) {
        let changes = self.values();
        let proposed = proposed_config(&self.config.borrow(), &changes);
        match proposed {
            Err(error) => self.status.set_label(&error.to_string()),
            Ok(_) => {
                self.status.set_label(if (self.save)(&changes) {
                    "Settings saved"
                } else {
                    "Could not apply settings. Stop active work, check the provider and try again."
                });
            }
        }
    }
}
