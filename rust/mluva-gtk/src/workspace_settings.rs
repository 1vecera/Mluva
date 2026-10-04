//! One atomic document/Live/recorder preference draft.
use crate::{
    appearance_settings::AppearanceSettings,
    prompt_editor::OpenEditor,
    settings_view::{SaveSettings, proposed_config},
};
use adw::prelude::*;
use mluva_core::{config::AppConfig, prompt_catalog::DEFAULTS};
use serde_json::{Map, Value};
use std::{cell::RefCell, collections::BTreeMap, rc::Rc};

enum Field {
    Switch(adw::SwitchRow),
    Spin(adw::SpinRow),
    Choice(adw::ComboRow, Vec<String>),
}
impl Field {
    fn read(&self) -> Value {
        match self {
            Self::Switch(row) => Value::Bool(row.is_active()),
            Self::Spin(row) => Value::from(row.value() as i64),
            Self::Choice(row, values) => Value::String(values[row.selected() as usize].clone()),
        }
    }
    fn write(&self, value: &Value) {
        match self {
            Self::Switch(row) => row.set_active(value.as_bool().unwrap()),
            Self::Spin(row) => row.set_value(value.as_f64().unwrap()),
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
    pub live_mode: adw::ComboRow,
    pub status: gtk::Label,
    pub apply_button: gtk::Button,
    config: RefCell<AppConfig>,
    fields: BTreeMap<String, Field>,
    save: SaveSettings,
}
impl WorkspaceSettings {
    pub fn new(
        config: &AppConfig,
        save: SaveSettings,
        edit_prompt: Option<OpenEditor>,
    ) -> Rc<Self> {
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
        let group = adw::PreferencesGroup::builder().title("Documents").build();
        for (name, title) in [
            (
                "auto_copy_dictation",
                "Copy completed dictation automatically",
            ),
            ("auto_copy_rewrite", "Copy completed rewrites automatically"),
            ("show_copy_action", "Show Copy icon"),
            ("show_save_action", "Show Save icon"),
            ("smooth_scrolling", "Smoothly follow new text"),
        ] {
            Self::switch(&group, &mut fields, &document, name, title);
        }
        for (name, title, lower, upper) in [
            (
                "review_timeout_seconds",
                "Widget dismissal delay (seconds)",
                1.0,
                60.0,
            ),
            (
                "scroll_duration_ms",
                "Scroll animation (milliseconds)",
                0.0,
                2000.0,
            ),
            (
                "scroll_lookahead_lines",
                "Space below new text (lines)",
                0.0,
                6.0,
            ),
        ] {
            Self::spin(&group, &mut fields, &document, name, title, lower, upper);
        }
        widget.add(&group);
        let live=adw::PreferencesGroup::builder().title("Live rewrite").description("Live rewrite sends provisional recognition to the model as speech arrives. Stop reconciles the draft with the final transcript. Later updates are grouped; short tails update after a pause. Batch speech engines update by chunk, not every word. Extra provider requests may use credits.").build();
        let live_mode = adw::ComboRow::builder()
            .title("Live rewrite mode")
            .model(&gtk::StringList::new(&["Off", "Once", "Continuous"]))
            .build();
        live_mode.set_selected(if !config.live_rewrite_enabled {
            0
        } else if config.live_rewrite_continuous {
            2
        } else {
            1
        });
        live.add(&live_mode);
        Self::choice(
            &live,
            &mut fields,
            &document,
            "live_rewrite_template",
            "Template",
            &DEFAULTS
                .live_templates
                .iter()
                .map(|template| (template.identifier.as_str(), template.name.as_str()))
                .collect::<Vec<_>>(),
        );
        Self::spin(
            &live,
            &mut fields,
            &document,
            "live_rewrite_min_characters",
            "New characters to group after the first draft",
            40.0,
            4000.0,
        );
        Self::spin(
            &live,
            &mut fields,
            &document,
            "live_rewrite_interval_seconds",
            "Minimum time between updates (seconds)",
            2.0,
            60.0,
        );
        let prompt_row = edit_prompt.map(|edit| {
            let row = adw::ActionRow::builder()
                .title("Edit Live prompt")
                .subtitle("All templates and structures are in Settings → Prompts.")
                .activatable(true)
                .build();
            live.add(&row);
            (row, edit)
        });
        widget.add(&live);
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
            live_mode,
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
        if let Some((row, edit)) = prompt_row {
            let weak = Rc::downgrade(&owner);
            row.connect_activated(move |_| {
                if let Some(owner) = weak.upgrade() {
                    let identifier =
                        format!("live-{}", owner.config.borrow().live_rewrite_template);
                    edit(&identifier);
                }
            });
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
    fn spin(
        group: &adw::PreferencesGroup,
        fields: &mut BTreeMap<String, Field>,
        document: &Value,
        name: &str,
        title: &str,
        lower: f64,
        upper: f64,
    ) {
        let adjustment = gtk::Adjustment::new(
            document[name].as_f64().unwrap(),
            lower,
            upper,
            1.0,
            10.0,
            0.0,
        );
        let row = adw::SpinRow::builder()
            .title(title)
            .adjustment(&adjustment)
            .build();
        group.add(&row);
        fields.insert(name.into(), Field::Spin(row));
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
        self.live_mode
            .set_selected(if !config.live_rewrite_enabled {
                0
            } else if config.live_rewrite_continuous {
                2
            } else {
                1
            });
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
        changes.insert(
            "live_rewrite_enabled".into(),
            Value::Bool(self.live_mode.selected() != 0),
        );
        changes.insert(
            "live_rewrite_continuous".into(),
            Value::Bool(self.live_mode.selected() == 2),
        );
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
