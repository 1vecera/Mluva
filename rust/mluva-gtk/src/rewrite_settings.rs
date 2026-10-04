//! Catalog-backed model, thinking and speed controls; discovery and persistence belong to the controller.

use adw::prelude::*;
use mluva_core::{config::AppConfig, text};
use mluva_providers::models::{Model, select_codex_model};
use std::cell::{Cell, RefCell};
use std::collections::BTreeMap;
use std::rc::Rc;
use std::sync::LazyLock;

pub type SaveRewriteSettings = Rc<dyn Fn(Option<String>, bool, Option<String>)>;

fn effort_label(level: &str) -> String {
    match level {
        "none" => "None".into(),
        "xhigh" => "Extra high".into(),
        _ => {
            static TITLE: LazyLock<BTreeMap<u32, String>> = LazyLock::new(|| {
                serde_json::from_str(include_str!("../resources/titlecase-first.json"))
                    .expect("frozen Unicode 16 titlecase expansions")
            });
            let source = level.replace('_', " ");
            let Some(first) = source.chars().next() else {
                return source;
            };
            let first = first.to_string();
            let lowered = text::lower(&source);
            let title = TITLE
                .get(&u32::from(first.chars().next().unwrap()))
                .cloned()
                .unwrap_or_else(|| text::upper(&first));
            title + &lowered[text::lower(&first).len()..]
        }
    }
}

pub struct ThinkingRow {
    pub widget: adw::ComboRow,
    choices: RefCell<Vec<Option<String>>>,
    labels: RefCell<Vec<String>>,
    updating: Cell<bool>,
    changed: Rc<dyn Fn(Option<String>)>,
}

impl ThinkingRow {
    pub fn new(changed: Rc<dyn Fn(Option<String>)>) -> Rc<Self> {
        let row = Rc::new(Self {
            widget: adw::ComboRow::builder().title("Thinking level").build(),
            choices: RefCell::new(vec![None]),
            labels: RefCell::new(vec![]),
            updating: Cell::new(false),
            changed,
        });
        let weak = Rc::downgrade(&row);
        row.widget.connect_selected_notify(move |widget| {
            if let Some(row) = weak.upgrade()
                && !row.updating.get()
            {
                let choice = row
                    .choices
                    .borrow()
                    .get(widget.selected() as usize)
                    .cloned();
                if let Some(choice) = choice {
                    (row.changed)(choice);
                }
            }
        });
        row
    }

    pub fn selected(&self) -> Option<String> {
        self.choices
            .borrow()
            .get(self.widget.selected() as usize)
            .cloned()
            .flatten()
    }

    pub fn configure(&self, config: &AppConfig, models: &[Model]) {
        let remote = config.rewrite_provider == "litellm";
        let (effort, requested) = if remote {
            (
                config.litellm_reasoning_effort.clone(),
                config.litellm_model.as_deref(),
            )
        } else {
            (
                config.rewrite_reasoning_effort.clone(),
                config
                    .rewrite_model
                    .as_deref()
                    .or(config.codex_model.as_deref()),
            )
        };
        let model = select_codex_model(models, requested).ok();
        let mut levels = model.map_or_else(Vec::new, |m| m.reasoning_efforts.clone());
        if remote && levels.is_empty() {
            levels = ["none", "minimal", "low", "medium", "high", "xhigh", "max"]
                .map(str::to_owned)
                .into();
        }
        self.updating.set(true);
        let mut choices = vec![None];
        choices.extend(levels.iter().cloned().map(Some));
        let mut labels = vec![if remote {
            "Provider default".into()
        } else {
            "Auto · low when supported".into()
        }];
        labels.extend(levels.iter().map(|level| effort_label(level)));
        if !choices.contains(&effort) {
            choices.push(effort.clone());
            labels.push(format!("{} · unavailable", effort.as_deref().unwrap()));
        }
        let selected = choices.iter().position(|choice| *choice == effort).unwrap();
        self.choices.replace(choices);
        if *self.labels.borrow() != labels {
            let strings = labels.iter().map(String::as_str).collect::<Vec<_>>();
            self.labels.replace(labels.clone());
            self.widget.set_model(Some(&gtk::StringList::new(&strings)));
        }
        self.widget.set_selected(selected as u32);
        self.widget.set_visible(config.rewrite_provider != "none");
        self.widget
            .set_sensitive(!levels.is_empty() || effort.is_some());
        self.widget.set_subtitle(
            if remote && model.is_none_or(|m| m.reasoning_efforts.is_empty()) {
                "Server support varies by model; default sends no override."
            } else if !levels.is_empty() {
                "Applies to rewrites and Live rewrite."
            } else if models.is_empty() {
                "Refresh models to check supported levels."
            } else {
                "This model advertises no thinking levels."
            },
        );
        self.updating.set(false);
    }
}

pub struct RewriteSettings {
    pub widget: gtk::MenuButton,
    pub caption: gtk::Label,
    pub popover: gtk::Popover,
    pub model_row: adw::ComboRow,
    pub thinking_row: Rc<ThinkingRow>,
    pub fast_row: adw::SwitchRow,
    pub status: gtk::Label,
    pub refresh: gtk::Button,
    config: RefCell<AppConfig>,
    models: RefCell<Vec<Model>>,
    choices: RefCell<Vec<Option<String>>>,
    labels: RefCell<Vec<String>>,
    updating: Cell<bool>,
    save: SaveRewriteSettings,
}

impl RewriteSettings {
    pub fn new(
        config: AppConfig,
        load_models: Rc<dyn Fn()>,
        save: SaveRewriteSettings,
    ) -> Rc<Self> {
        let caption = gtk::Label::builder()
            .ellipsize(gtk::pango::EllipsizeMode::End)
            .max_width_chars(18)
            .build();
        let heading = gtk::Box::new(gtk::Orientation::Horizontal, 6);
        heading.append(&caption);
        heading.append(&gtk::Image::from_icon_name("pan-down-symbolic"));
        let widget = gtk::MenuButton::builder().child(&heading).build();
        let popover = gtk::Popover::new();
        let content = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .width_request(300)
            .build();
        crate::document_layout::margins(&content, 12);
        let group = adw::PreferencesGroup::builder().title("Rewriting").build();
        let model_row = adw::ComboRow::builder().title("Codex model").build();
        group.add(&model_row);
        let settings = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let weak = weak.clone();
            let thinking_row = ThinkingRow::new(Rc::new(move |_| {
                if let Some(settings) = weak.upgrade() {
                    settings.changed();
                }
            }));
            group.add(&thinking_row.widget);
            let fast_row = adw::SwitchRow::builder().title("Fast mode").build();
            group.add(&fast_row);
            content.append(&group);
            let status = gtk::Label::builder()
                .xalign(0.0)
                .wrap(true)
                .max_width_chars(34)
                .build();
            status.add_css_class("caption");
            content.append(&status);
            let refresh = gtk::Button::builder()
                .label("Refresh models")
                .halign(gtk::Align::Start)
                .build();
            content.append(&refresh);
            popover.set_child(Some(&content));
            widget.set_popover(Some(&popover));
            Self {
                widget,
                caption,
                popover,
                model_row,
                thinking_row,
                fast_row,
                status,
                refresh,
                config: RefCell::new(config.clone()),
                models: RefCell::new(vec![]),
                choices: RefCell::new(vec![]),
                labels: RefCell::new(vec![]),
                updating: Cell::new(false),
                save,
            }
        });
        let load = load_models.clone();
        settings.refresh.connect_clicked(move |_| load());
        settings.popover.connect_show(move |_| load_models());
        let weak = Rc::downgrade(&settings);
        settings.model_row.connect_selected_notify(move |_| {
            if let Some(settings) = weak.upgrade() {
                settings.changed();
            }
        });
        let weak = Rc::downgrade(&settings);
        settings.fast_row.connect_active_notify(move |_| {
            if let Some(settings) = weak.upgrade() {
                settings.changed();
            }
        });
        settings.set_config(config);
        settings
    }

    pub fn set_config(&self, config: AppConfig) {
        self.widget.set_sensitive(config.rewrite_provider != "none");
        let remote = config.rewrite_provider == "litellm";
        let previous = self.config.replace(config.clone());
        if previous.rewrite_provider != config.rewrite_provider
            || previous.litellm_base_url != config.litellm_base_url
            || previous.litellm_api_key_env != config.litellm_api_key_env
        {
            self.models.borrow_mut().clear();
            self.refresh.set_sensitive(true);
            self.status.set_label("");
        }
        let models = self.models.borrow().clone();
        self.thinking_row.configure(&config, &models);
        self.model_row.set_title(if remote {
            "Server model"
        } else {
            "Codex model"
        });
        self.fast_row.set_visible(!remote);
        self.widget
            .set_tooltip_text(Some("Choose the rewrite model and speed"));
        let mut display = config;
        if remote {
            display.rewrite_model = display.litellm_model.clone();
            display.codex_model = display.litellm_model.clone();
            display.rewrite_fast_mode = false;
        }
        self.updating.set(true);
        let mut selected = display.rewrite_model.clone();
        let (mut choices, mut labels) = if remote && selected.is_some() {
            (vec![], vec![])
        } else {
            (
                vec![None],
                vec![if remote {
                    "Choose in Settings → Providers".into()
                } else {
                    "Default".into()
                }],
            )
        };
        for model in &models {
            if !model.hidden
                || selected
                    .as_ref()
                    .is_some_and(|id| id == &model.id || id == &model.identifier)
            {
                choices.push(Some(model.identifier.clone()));
                labels.push(model.name.clone());
                if selected.as_ref() == Some(&model.id) {
                    selected = Some(model.identifier.clone());
                }
            }
        }
        if !choices.contains(&selected) {
            choices.push(selected.clone());
            let id = selected.as_deref().unwrap();
            labels.push(if !models.is_empty() {
                format!(
                    "{id} ({})",
                    if remote { "not listed" } else { "unavailable" }
                )
            } else {
                id.into()
            });
        }
        let changed = *self.choices.borrow() != choices || *self.labels.borrow() != labels;
        self.choices.replace(choices.clone());
        if changed {
            let strings = labels.iter().map(String::as_str).collect::<Vec<_>>();
            self.labels.replace(labels.clone());
            self.model_row
                .set_model(Some(&gtk::StringList::new(&strings)));
        }
        self.model_row.set_selected(
            choices
                .iter()
                .position(|choice| *choice == selected)
                .unwrap() as u32,
        );
        self.model_row.set_sensitive(!models.is_empty());
        let effective = select_codex_model(
            &models,
            display
                .rewrite_model
                .as_deref()
                .or(display.codex_model.as_deref()),
        )
        .ok();
        let fast = effective.is_some_and(|model| model.fast_tier.is_some());
        self.fast_row
            .set_sensitive(fast || display.rewrite_fast_mode);
        self.fast_row.set_active(display.rewrite_fast_mode);
        self.fast_row.set_subtitle(if fast {
            "Uses more Codex credits"
        } else if !models.is_empty() {
            "Unavailable for this model"
        } else {
            "Load models to check availability"
        });
        self.model_row
            .set_subtitle(&match (selected.as_ref(), effective) {
                (None, Some(model)) => format!("Default: {}", model.name),
                _ => "Applies to rewrites only".into(),
            });
        let label = effective.map_or_else(
            || {
                display.rewrite_model.clone().unwrap_or_else(|| {
                    if remote {
                        "Choose rewrite model".into()
                    } else {
                        "Codex model".into()
                    }
                })
            },
            |model| model.name.clone(),
        );
        self.caption.set_label(&if display.rewrite_fast_mode {
            format!("{label} · Fast")
        } else {
            label
        });
        self.updating.set(false);
    }

    pub fn set_loading(&self) {
        self.refresh.set_sensitive(false);
        self.status.set_label("Loading models…");
    }

    pub fn reset_catalog(&self) {
        self.models.borrow_mut().clear();
        let config = self.config.borrow().clone();
        self.set_config(config);
    }

    pub fn set_models(&self, models: Option<Vec<Model>>) {
        self.refresh.set_sensitive(true);
        let Some(models) = models.filter(|models| !models.is_empty()) else {
            self.status
                .set_label("Could not load models. Check your provider, then refresh.");
            return;
        };
        self.models.replace(models);
        let config = self.config.borrow().clone();
        self.set_config(config.clone());
        self.status.set_label(if config.rewrite_provider == "litellm" { "Select an available deployment. You can also enter its alias in Settings → Providers." } else { "Thinking levels follow the selected model. Auto uses low reasoning when supported." });
    }

    fn changed(&self) {
        if self.updating.get() {
            return;
        }
        let selected = self
            .choices
            .borrow()
            .get(self.model_row.selected() as usize)
            .cloned();
        let Some(selected) = selected else {
            return;
        };
        let mut effort = self.thinking_row.selected();
        let config = self.config.borrow().clone();
        let models = self.models.borrow().clone();
        let mut previous = if config.rewrite_provider == "litellm" {
            config.litellm_model.clone()
        } else {
            config.rewrite_model.clone()
        };
        if let Some(id) = &previous
            && let Ok(model) = select_codex_model(&models, Some(id))
        {
            previous = Some(model.identifier.clone());
        }
        if selected != previous {
            effort = None;
        }
        let fast = select_codex_model(
            &models,
            selected.as_deref().or(config.codex_model.as_deref()),
        )
        .is_ok_and(|model| self.fast_row.is_active() && model.fast_tier.is_some());
        if config.rewrite_provider == "litellm" {
            if (selected.clone(), effort.clone())
                != (config.litellm_model, config.litellm_reasoning_effort)
            {
                (self.save)(selected, false, effort);
            }
        } else if (selected.clone(), fast, effort.clone())
            != (
                config.rewrite_model,
                config.rewrite_fast_mode,
                config.rewrite_reasoning_effort,
            )
        {
            (self.save)(selected, fast, effort);
        }
    }
}
