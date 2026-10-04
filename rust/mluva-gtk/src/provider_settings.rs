//! Provider drafts, guarded discovery and secure key saves on the GTK owner.
use crate::{
    async_runtime::DesktopRuntime,
    direct_choices::DirectChoices,
    local_model_settings::LocalModelSettings,
    rewrite_settings::ThinkingRow,
    settings_view::{SaveSettings, proposed_config},
};
use adw::prelude::*;
use mluva_core::{config::AppConfig, text};
use mluva_providers::{
    catalog::{CatalogClient, CatalogRequest, Provider, Scope, connection_hint},
    credentials::CredentialStore,
    models::{Model, select_codex_model, valid_identifier},
};
use serde_json::{Map, Value};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
    sync::Arc,
};

#[derive(Clone, PartialEq, Eq)]
enum ModelChoice {
    Default,
    Identifier(String),
    Manual,
}
pub struct ProviderSection {
    pub widget: adw::PreferencesGroup,
    pub scope: Scope,
    pub provider_row: Rc<DirectChoices>,
    pub description: gtk::Label,
    pub api_key_entry: adw::PasswordEntryRow,
    pub key_hint: gtk::Label,
    pub fixed_model_row: adw::ActionRow,
    pub model_row: adw::ComboRow,
    pub model_entry: adw::EntryRow,
    pub fast_row: adw::SwitchRow,
    pub thinking_row: Rc<ThinkingRow>,
    pub connection: adw::ActionRow,
    pub refresh_button: gtk::Button,
    pub advanced: adw::ExpanderRow,
    pub endpoint_entry: adw::EntryRow,
    pub key_entry: adw::EntryRow,
    pub preview: adw::ExpanderRow,
    pub chunk_row: adw::SpinRow,
    pub status: gtk::Label,
    pub local: Rc<LocalModelSettings>,
    runtime: Rc<DesktopRuntime>,
    config: RefCell<AppConfig>,
    values: RefCell<Map<String, Value>>,
    models: RefCell<Vec<Model>>,
    catalog_loaded: Cell<bool>,
    client: RefCell<Option<Arc<CatalogClient>>>,
    choices: RefCell<Vec<ModelChoice>>,
    updating: Cell<bool>,
}
impl ProviderSection {
    pub fn new(
        config: &AppConfig,
        scope: Scope,
        data: PathBuf,
        runtime: Rc<DesktopRuntime>,
    ) -> Rc<Self> {
        let widget = adw::PreferencesGroup::builder()
            .title(if scope == Scope::Speech {
                "Speech recognition"
            } else {
                "Rewriting"
            })
            .build();
        let owner = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let target = weak.clone();
            let labels = if scope == Scope::Speech {
                ["ElevenLabs", "Local model", "Custom"]
            } else {
                ["Codex", "Skip", "Custom"]
            };
            let provider_row = DirectChoices::new(
                &labels,
                Rc::new(move |_| {
                    if let Some(owner) = target.upgrade() {
                        owner.provider_changed();
                    }
                }),
            );
            let choice = gtk::ListBoxRow::builder()
                .activatable(false)
                .selectable(false)
                .child(&provider_row.widget)
                .build();
            widget.add(&choice);
            let description = gtk::Label::builder()
                .xalign(0.0)
                .wrap(true)
                .margin_bottom(12)
                .build();
            description.add_css_class("caption");
            widget.add(&description);
            let api_key_entry = adw::PasswordEntryRow::builder()
                .title("ElevenLabs API key")
                .build();
            widget.add(&api_key_entry);
            let key_hint = gtk::Label::builder()
                .label("Saved securely in your desktop keyring when you continue or apply.")
                .xalign(0.0)
                .wrap(true)
                .margin_top(8)
                .build();
            key_hint.add_css_class("caption");
            widget.add(&key_hint);
            let fixed_model_row = adw::ActionRow::builder()
                .title("Model")
                .subtitle("Scribe v2 · live recognition")
                .build();
            widget.add(&fixed_model_row);
            let model_row = adw::ComboRow::builder()
                .title("Model")
                .enable_search(true)
                .use_subtitle(true)
                .subtitle_lines(0)
                .build();
            widget.add(&model_row);
            let model_entry = adw::EntryRow::builder()
                .title("Model ID / deployment alias")
                .build();
            widget.add(&model_entry);
            let fast_row = adw::SwitchRow::builder().title("Fast mode").build();
            widget.add(&fast_row);
            let target = weak.clone();
            let thinking_row = ThinkingRow::new(Rc::new(move |effort| {
                if let Some(owner) = target.upgrade()
                    && !owner.updating.get()
                {
                    let field = if owner.provider().id == "litellm" {
                        "litellm_reasoning_effort"
                    } else {
                        "rewrite_reasoning_effort"
                    };
                    owner
                        .values
                        .borrow_mut()
                        .insert(field.into(), effort.map_or(Value::Null, Value::String));
                }
            }));
            widget.add(&thinking_row.widget);
            let connection = adw::ActionRow::builder()
                .title("Connection")
                .subtitle_lines(0)
                .build();
            let refresh_button = gtk::Button::builder()
                .label("Refresh")
                .valign(gtk::Align::Center)
                .tooltip_text("Refresh model availability without sending audio or text")
                .build();
            connection.add_suffix(&refresh_button);
            widget.add(&connection);
            let advanced = adw::ExpanderRow::builder()
                .title("Connection details")
                .subtitle("Endpoint and key variable name")
                .build();
            let endpoint_entry = adw::EntryRow::builder()
                .title("API base URL (include the API prefix)")
                .build();
            let key_entry = adw::EntryRow::builder()
                .title("API key environment variable · name only")
                .build();
            advanced.add_row(&endpoint_entry);
            advanced.add_row(&key_entry);
            advanced.add_row(&adw::ActionRow::builder().title("Keep keys in your secret manager").subtitle("Use HTTPS, or HTTP for a local server. Never put a key in the URL or these fields.").build());
            widget.add(&advanced);
            let preview = adw::ExpanderRow::builder()
                .title("Live speech previews")
                .build();
            let chunk_row = adw::SpinRow::builder()
                .title("Seconds per audio chunk")
                .adjustment(&gtk::Adjustment::new(0.0, 3.0, 30.0, 1.0, 5.0, 0.0))
                .build();
            preview.add_row(&chunk_row);
            preview.add_row(&adw::ActionRow::builder().title("Batch providers recognize again at Stop").subtitle("Shorter chunks update sooner but make more requests. Cloud previews can increase usage.").build());
            widget.add(&preview);
            let status = gtk::Label::builder()
                .xalign(0.0)
                .wrap(true)
                .margin_top(8)
                .margin_bottom(8)
                .build();
            status.add_css_class("caption");
            widget.add(&status);
            let target = weak.clone();
            let language_target = weak.clone();
            let local = LocalModelSettings::new(
                data,
                runtime.clone(),
                &config.local_model,
                &config.local_device,
                &config.language_code,
                Rc::new(move |identifier| {
                    if let Some(owner) = target.upgrade()
                        && owner.scope == Scope::Speech
                    {
                        let mut values = owner.values.borrow_mut();
                        values.insert("local_model".into(), Value::String(identifier.into()));
                        values.insert(
                            "local_device".into(),
                            Value::String(owner.local.device().into()),
                        );
                    }
                }),
                Rc::new(move |language| {
                    if let Some(owner) = language_target.upgrade()
                        && owner.scope == Scope::Speech
                    {
                        owner
                            .values
                            .borrow_mut()
                            .insert("language_code".into(), Value::String(language.into()));
                    }
                }),
            );
            if scope == Scope::Speech {
                widget.add(&local.widget);
            }
            Self {
                widget,
                scope,
                provider_row,
                description,
                api_key_entry,
                key_hint,
                fixed_model_row,
                model_row,
                model_entry,
                fast_row,
                thinking_row,
                connection,
                refresh_button,
                advanced,
                endpoint_entry,
                key_entry,
                preview,
                chunk_row,
                status,
                local,
                runtime,
                config: RefCell::new(config.clone()),
                values: RefCell::new(Map::new()),
                models: RefCell::new(Vec::new()),
                catalog_loaded: Cell::new(false),
                client: RefCell::new(None),
                choices: RefCell::new(Vec::new()),
                updating: Cell::new(true),
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.model_row.connect_selected_notify(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.model_changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.model_entry.connect_changed(move |_| {
            if let Some(owner) = weak.upgrade()
                && !owner.updating.get()
            {
                let value = text::trim(&owner.model_entry.text()).to_owned();
                owner.values.borrow_mut().insert(
                    owner.provider().model_field.into(),
                    if value.is_empty() {
                        Value::Null
                    } else {
                        Value::String(value)
                    },
                );
                owner.show_fast(true);
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.fast_row.connect_active_notify(move |row| {
            if let Some(owner) = weak.upgrade()
                && !owner.updating.get()
            {
                owner
                    .values
                    .borrow_mut()
                    .insert("rewrite_fast_mode".into(), Value::Bool(row.is_active()));
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.chunk_row.connect_value_notify(move |row| {
            if let Some(owner) = weak.upgrade()
                && !owner.updating.get()
                && owner.scope == Scope::Speech
            {
                owner.values.borrow_mut().insert(
                    "transcription_chunk_seconds".into(),
                    Value::from(row.value() as i64),
                );
            }
        });
        for (entry, field) in [
            (&owner.endpoint_entry, scope.base_field()),
            (&owner.key_entry, scope.key_field()),
        ] {
            let weak = Rc::downgrade(&owner);
            entry.connect_changed(move |entry| {
                if let Some(owner) = weak.upgrade()
                    && !owner.updating.get()
                {
                    owner.values.borrow_mut().insert(
                        field.into(),
                        Value::String(text::trim(&entry.text()).into()),
                    );
                    owner.reset_catalog();
                }
            });
        }
        let weak = Rc::downgrade(&owner);
        owner.refresh_button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.load_models();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.widget.connect_unmap(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.stop_lookup();
            }
        });
        owner.refresh_config(config);
        owner
    }
    pub fn provider(&self) -> &'static Provider {
        &self.scope.providers()[self.provider_row.selected()]
    }
    pub fn values(&self) -> Map<String, Value> {
        self.values.borrow().clone()
    }
    pub fn is_ready(&self) -> bool {
        self.provider().id != "local" || self.local.is_ready()
    }
    fn draft_config(&self) -> AppConfig {
        let mut document = serde_json::to_value(&*self.config.borrow()).unwrap();
        document.as_object_mut().unwrap().extend(self.values());
        serde_json::from_value(document).expect("typed provider controls")
    }
    pub fn refresh_config(&self, config: &AppConfig) {
        self.stop_lookup();
        self.api_key_entry.set_text("");
        self.config.replace(config.clone());
        self.local.stop();
        self.local
            .languages
            .refresh(&config.language_code, &config.local_model);
        self.local
            .set_selection(&config.local_model, &config.local_device);
        let document = serde_json::to_value(config).unwrap();
        let mut names = vec![
            self.scope.provider_field(),
            self.scope.base_field(),
            self.scope.key_field(),
        ];
        names.extend(
            self.scope
                .providers()
                .iter()
                .map(|provider| provider.model_field),
        );
        if self.scope == Scope::Speech {
            names.extend([
                "local_device",
                "language_code",
                "transcription_chunk_seconds",
            ]);
        } else {
            names.extend([
                "rewrite_reasoning_effort",
                "litellm_reasoning_effort",
                "rewrite_fast_mode",
            ]);
        }
        self.values.replace(
            names
                .into_iter()
                .map(|name| (name.into(), document[name].clone()))
                .collect(),
        );
        self.updating.set(true);
        self.provider_row.set_selected(
            self.scope
                .providers()
                .iter()
                .position(|provider| {
                    Some(provider.id) == document[self.scope.provider_field()].as_str()
                })
                .unwrap(),
        );
        self.endpoint_entry
            .set_text(document[self.scope.base_field()].as_str().unwrap());
        self.key_entry
            .set_text(document[self.scope.key_field()].as_str().unwrap());
        self.chunk_row
            .set_value(config.transcription_chunk_seconds as f64);
        self.updating.set(false);
        self.reset_catalog();
    }
    fn provider_changed(&self) {
        if !self.updating.get() {
            self.values.borrow_mut().insert(
                self.scope.provider_field().into(),
                Value::String(self.provider().id.into()),
            );
            self.reset_catalog();
            self.local.stop();
            self.local.refresh();
        }
    }
    fn reset_catalog(&self) {
        self.stop_lookup();
        self.models.borrow_mut().clear();
        self.catalog_loaded.set(false);
        let provider = self.provider();
        self.description.set_label(provider.description);
        self.api_key_entry.set_visible(provider.id == "elevenlabs");
        self.key_hint.set_visible(provider.id == "elevenlabs");
        self.local
            .widget
            .set_visible(self.scope == Scope::Speech && provider.id == "local");
        self.advanced.set_visible(provider.id == "litellm");
        self.preview
            .set_visible(self.scope == Scope::Speech && provider.id == "litellm");
        self.fast_row.set_visible(false);
        self.refresh_button
            .set_visible(!["elevenlabs", "none", "local"].contains(&provider.id));
        self.fixed_model_row.set_visible(false);
        self.model_row
            .set_visible(["codex", "litellm"].contains(&provider.id));
        self.connection
            .set_visible(["codex", "litellm"].contains(&provider.id));
        self.connection.set_subtitle(connection_hint(
            provider.id,
            self.values.borrow()[self.scope.key_field()]
                .as_str()
                .unwrap(),
        ));
        self.status.set_label("");
        self.status.set_visible(false);
        if provider.id == "elevenlabs" {
            self.models.replace(vec![Model {
                id: "scribe_v2".into(),
                identifier: "scribe_v2".into(),
                name: "Scribe v2".into(),
                is_default: true,
                hidden: false,
                rewrite_effort: None,
                fast_tier: None,
                reasoning_efforts: Vec::new(),
            }]);
        }
        self.show_models();
        if provider.id != "litellm" {
            self.model_entry.set_visible(false);
        }
    }
    fn show_models(&self) {
        self.updating.set(true);
        let selected = self.values.borrow()[self.provider().model_field]
            .as_str()
            .map(str::to_owned);
        let selected_choice = selected
            .clone()
            .map_or(ModelChoice::Default, ModelChoice::Identifier);
        let mut choices = Vec::new();
        let mut labels = Vec::new();
        if let Some(default) = self.provider().default_label {
            choices.push(ModelChoice::Default);
            let mut label = default.to_owned();
            if self.provider().id == "codex"
                && let Some(model) = &self.config.borrow().codex_model
            {
                label.push_str(&format!(" ({model})"));
            }
            labels.push(label);
        }
        let models = self.models.borrow();
        let visible: Vec<_> = models
            .iter()
            .filter(|model| {
                !model.hidden
                    || selected.as_deref() == Some(&model.id)
                    || selected.as_deref() == Some(&model.identifier)
            })
            .collect();
        let current = visible.iter().copied().find(|model| {
            selected.as_deref() == Some(&model.id) || selected.as_deref() == Some(&model.identifier)
        });
        if let Some(selected) = &selected {
            choices.push(ModelChoice::Identifier(selected.clone()));
            labels.push(current.map_or_else(
                || {
                    if self.catalog_loaded.get() {
                        format!("{selected} · not listed")
                    } else {
                        selected.clone()
                    }
                },
                |model| model.name.clone(),
            ));
        }
        for model in visible {
            if current.is_none_or(|current| !std::ptr::eq(current, model))
                && !choices.contains(&ModelChoice::Identifier(model.identifier.clone()))
            {
                choices.push(ModelChoice::Identifier(model.identifier.clone()));
                labels.push(model.name.clone());
            }
        }
        if self.provider().id != "elevenlabs" {
            choices.push(ModelChoice::Manual);
            labels.push("Enter model ID…".into());
        }
        let index = choices
            .iter()
            .position(|choice| *choice == selected_choice)
            .or_else(|| {
                choices
                    .iter()
                    .position(|choice| *choice == ModelChoice::Manual)
            })
            .unwrap();
        let manual = choices[index] == ModelChoice::Manual;
        self.choices.replace(choices);
        let labels: Vec<_> = labels.iter().map(String::as_str).collect();
        self.model_row
            .set_model(Some(&gtk::StringList::new(&labels)));
        self.model_row.set_selected(index as u32);
        self.model_row
            .set_sensitive(self.provider().id != "elevenlabs");
        self.model_entry.set_text(selected.as_deref().unwrap_or(""));
        self.model_entry.set_visible(manual);
        self.updating.set(false);
        drop(models);
        self.show_fast(false);
    }
    fn model_changed(&self) {
        if self.updating.get() {
            return;
        }
        let Some(choice) = self
            .choices
            .borrow()
            .get(self.model_row.selected() as usize)
            .cloned()
        else {
            return;
        };
        let manual = choice == ModelChoice::Manual;
        self.model_entry.set_visible(manual);
        if !manual {
            let value = match choice {
                ModelChoice::Default => Value::Null,
                ModelChoice::Identifier(value) => Value::String(value),
                ModelChoice::Manual => unreachable!(),
            };
            self.values
                .borrow_mut()
                .insert(self.provider().model_field.into(), value);
            self.show_fast(true);
        } else {
            self.updating.set(true);
            self.model_entry.set_text(
                self.values.borrow()[self.provider().model_field]
                    .as_str()
                    .unwrap_or(""),
            );
            self.updating.set(false);
            self.model_entry.grab_focus();
        }
    }
    fn show_fast(&self, reset: bool) {
        if self.scope == Scope::Rewrite {
            if reset {
                let field = if self.provider().id == "litellm" {
                    "litellm_reasoning_effort"
                } else {
                    "rewrite_reasoning_effort"
                };
                self.values.borrow_mut().insert(field.into(), Value::Null);
            }
            self.thinking_row
                .configure(&self.draft_config(), &self.models.borrow());
        } else {
            self.thinking_row.widget.set_visible(false);
        }
        self.fast_row
            .set_visible(self.scope == Scope::Rewrite && self.provider().id == "codex");
        if self.provider().id != "codex" {
            return;
        }
        let config = self.config.borrow();
        let values = self.values.borrow();
        let selected = values["rewrite_model"]
            .as_str()
            .filter(|value| !value.is_empty())
            .or(config.codex_model.as_deref());
        let supported = select_codex_model(&self.models.borrow(), selected)
            .is_ok_and(|model| model.fast_tier.is_some());
        drop(values);
        drop(config);
        if reset && !supported {
            self.values
                .borrow_mut()
                .insert("rewrite_fast_mode".into(), Value::Bool(false));
        }
        self.updating.set(true);
        let active = self.values.borrow()["rewrite_fast_mode"].as_bool().unwrap();
        self.fast_row.set_active(active);
        self.fast_row.set_sensitive(supported || active);
        self.fast_row.set_subtitle(if supported {
            "Uses more Codex credits"
        } else if self.catalog_loaded.get() {
            "Unavailable for this model"
        } else {
            "Refresh models to check Fast availability"
        });
        self.updating.set(false);
    }
    pub fn load_models(self: &Rc<Self>) {
        if self.client.borrow().is_some() {
            return;
        }
        let request = CatalogRequest {
            scope: self.scope,
            provider: self.provider().id.into(),
            base_url: self.values.borrow()[self.scope.base_field()]
                .as_str()
                .unwrap()
                .into(),
            key_env: self.values.borrow()[self.scope.key_field()]
                .as_str()
                .unwrap()
                .into(),
        };
        let client = proposed_config(&self.config.borrow(), &self.values())
            .ok()
            .and_then(|_| request.client().ok());
        let Some(client) = client else {
            self.status.set_label(
                "Check the model ID, endpoint and key variable name in Connection details.",
            );
            self.status.set_visible(true);
            return;
        };
        let client = Arc::new(client);
        self.client.replace(Some(client.clone()));
        self.refresh_button.set_sensitive(false);
        self.status
            .set_label("Loading models… Your selection will stay unchanged.");
        self.status.set_visible(true);
        let transport = client.clone();
        let route = request.clone();
        let task = self.runtime.spawn_background(async move {
            let models = transport.read(&route).await.ok();
            transport.close().await;
            models
        });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            let models = task.await.ok().flatten();
            if let Some(owner) = weak.upgrade()
                && owner
                    .client
                    .borrow()
                    .as_ref()
                    .is_some_and(|current| Arc::ptr_eq(current, &client))
            {
                owner.client.borrow_mut().take();
                owner.refresh_button.set_sensitive(true);
                let message = request.message(models.as_deref());
                if let Some(models) = models {
                    owner.models.replace(models);
                    owner.catalog_loaded.set(true);
                    owner.show_models();
                }
                owner.status.set_label(&message);
                owner.status.set_visible(true);
            }
        });
    }
    pub fn stop_lookup(&self) {
        let client = self.client.borrow_mut().take();
        self.refresh_button.set_sensitive(true);
        if let Some(client) = client {
            self.status
                .set_label("Model refresh stopped. Your choice is kept.");
            client.cancel();
        }
    }
    fn accept_config(&self, config: &AppConfig) {
        self.config.replace(config.clone());
    }
}
impl Drop for ProviderSection {
    fn drop(&mut self) {
        self.stop_lookup();
        self.local.stop();
    }
}

pub struct ProviderSettings {
    pub widget: adw::PreferencesPage,
    pub speech: Rc<ProviderSection>,
    pub rewrite: Rc<ProviderSection>,
    pub apply_button: gtk::Button,
    pub status: gtk::Label,
    config: RefCell<AppConfig>,
    save: SaveSettings,
    credentials: Arc<CredentialStore>,
    runtime: Rc<DesktopRuntime>,
    saving_key: Cell<bool>,
}
impl ProviderSettings {
    pub fn new(
        config: &AppConfig,
        data: PathBuf,
        runtime: Rc<DesktopRuntime>,
        credentials: Arc<CredentialStore>,
        save: SaveSettings,
        introduction: Option<&gtk::Widget>,
    ) -> Rc<Self> {
        let widget = adw::PreferencesPage::builder()
            .name("providers")
            .title("Providers")
            .icon_name("network-server-symbolic")
            .build();
        if let Some(introduction) = introduction {
            let group = adw::PreferencesGroup::new();
            group.add(introduction);
            widget.add(&group);
        }
        let speech = ProviderSection::new(config, Scope::Speech, data.clone(), runtime.clone());
        let rewrite = ProviderSection::new(config, Scope::Rewrite, data, runtime.clone());
        widget.add(&speech.widget);
        widget.add(&rewrite.widget);
        let actions = adw::PreferencesGroup::new();
        let row = adw::ActionRow::builder()
            .title("Save provider choices")
            .subtitle("Applies to the next recording or request.")
            .build();
        let apply_button = gtk::Button::builder()
            .label("Apply")
            .valign(gtk::Align::Center)
            .build();
        apply_button.add_css_class("suggested-action");
        row.add_suffix(&apply_button);
        actions.add(&row);
        let status = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .margin_top(8)
            .build();
        actions.add(&status);
        widget.add(&actions);
        let owner = Rc::new(Self {
            widget,
            speech,
            rewrite,
            apply_button,
            status,
            config: RefCell::new(config.clone()),
            save,
            credentials,
            runtime,
            saving_key: Cell::new(false),
        });
        let weak = Rc::downgrade(&owner);
        owner.apply_button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.apply();
            }
        });
        owner
    }
    pub fn refresh_config(&self, config: &AppConfig) {
        self.config.replace(config.clone());
        self.speech.refresh_config(config);
        self.rewrite.refresh_config(config);
        self.status.set_label("");
    }
    pub fn apply(self: &Rc<Self>) {
        if self.saving_key.get() {
            return;
        }
        if !self.speech.is_ready() {
            self.status
                .set_label("Finish downloading the selected local model before applying.");
            return;
        }
        for section in [&self.speech, &self.rewrite] {
            let values = section.values();
            let model = values[section.provider().model_field].as_str();
            if model.is_some_and(|model| !valid_identifier(model))
                || (section.provider().id == "litellm" && model.is_none_or(str::is_empty))
            {
                self.status.set_label(&format!(
                    "Choose a {} model or enter its deployment ID.",
                    section.scope.name()
                ));
                return;
            }
        }
        let mut changes = self.speech.values();
        changes.extend(self.rewrite.values());
        let Ok(config) = proposed_config(&self.config.borrow(), &changes) else {
            self.status.set_label(
                "Check Connection details. Use a plain base URL and an environment variable name.",
            );
            return;
        };
        let key = text::trim(&self.speech.api_key_entry.text()).to_owned();
        if self.speech.provider().id == "elevenlabs" && !key.is_empty() {
            self.speech.api_key_entry.set_text("");
            self.saving_key.set(true);
            self.widget.set_sensitive(false);
            self.status.set_label("Saving your key…");
            let credentials = self.credentials.clone();
            let weak = Rc::downgrade(self);
            self.runtime.spawn(async move {
                let error = credentials
                    .store_speech_key(&key)
                    .await
                    .err()
                    .map(|error| error.to_string());
                if let Some(owner) = weak.upgrade() {
                    owner.saving_key.set(false);
                    owner.widget.set_sensitive(true);
                    if let Some(error) = error {
                        owner.status.set_label(&error);
                    } else {
                        owner.apply();
                    }
                }
            });
            return;
        }
        if (self.save)(&changes) {
            self.config.replace(config.clone());
            self.speech.accept_config(&config);
            self.rewrite.accept_config(&config);
            self.status
                .set_label("Provider choices saved. They apply to the next recording or request.");
        } else {
            self.status
                .set_label("Could not save. Stop active work and try again; your edits are kept.");
        }
    }
}
