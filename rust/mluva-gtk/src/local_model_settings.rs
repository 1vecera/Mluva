//! Verified native model provisioning with selection-owned cancellation and progress.
use crate::{async_runtime::DesktopRuntime, language_picker::LanguagePicker};
use adw::prelude::*;
use mluva_providers::{
    languages,
    local_assets::{AssetStore, MODEL_CATALOG, QWEN_RUNTIME, model},
    onnx_runtime::{ONNX_RUNTIME, gpu_name},
};
use std::{
    cell::{Cell, RefCell},
    path::PathBuf,
    rc::Rc,
};
use tokio_util::sync::CancellationToken;

pub struct LocalModelSettings {
    pub widget: gtk::Box,
    pub label: gtk::Label,
    pub scale: gtk::Scale,
    pub gpu: gtk::CheckButton,
    pub languages: Rc<LanguagePicker>,
    pub requirements: gtk::Label,
    pub progress: gtk::ProgressBar,
    pub button: gtk::Button,
    pub status: gtk::Label,
    data: PathBuf,
    runtime: Rc<DesktopRuntime>,
    changed: Rc<dyn Fn(&str)>,
    updating: Cell<bool>,
    generation: Cell<u64>,
    cancellation: RefCell<Option<CancellationToken>>,
}
impl LocalModelSettings {
    pub fn new(
        data: PathBuf,
        runtime: Rc<DesktopRuntime>,
        selected: &str,
        device: &str,
        language: &str,
        changed: Rc<dyn Fn(&str)>,
        language_changed: Rc<dyn Fn(&str)>,
    ) -> Rc<Self> {
        let widget = gtk::Box::builder()
            .orientation(gtk::Orientation::Vertical)
            .spacing(8)
            .margin_top(12)
            .margin_bottom(12)
            .build();
        let label = gtk::Label::builder().xalign(0.0).wrap(true).build();
        widget.append(&label);
        let offered: Vec<_> = MODEL_CATALOG.iter().filter(|model| model.offered).collect();
        let scale = gtk::Scale::with_range(
            gtk::Orientation::Horizontal,
            0.0,
            (offered.len() - 1) as f64,
            1.0,
        );
        scale.set_draw_value(false);
        scale.set_round_digits(0);
        scale.set_value(
            offered
                .iter()
                .position(|model| model.id == selected)
                .unwrap_or(offered.len() - 1) as f64,
        );
        scale.update_property(&[gtk::accessible::Property::Label(
            "Local model size, smallest to largest",
        )]);
        for index in 0..offered.len() {
            scale.add_mark(index as f64, gtk::PositionType::Bottom, None);
        }
        widget.append(&scale);
        let ends = gtk::Box::new(gtk::Orientation::Horizontal, 0);
        ends.append(
            &gtk::Label::builder()
                .label("Smaller / lighter")
                .xalign(0.0)
                .hexpand(true)
                .build(),
        );
        ends.append(
            &gtk::Label::builder()
                .label("Larger / heavier")
                .xalign(1.0)
                .build(),
        );
        widget.append(&ends);
        let gpu = gtk::CheckButton::builder()
            .label("Use NVIDIA GPU")
            .active(device == "cuda")
            .visible(device == "cuda")
            .tooltip_text("NVIDIA GPU unavailable on this computer")
            .build();
        widget.append(&gpu);
        let owner = Rc::new_cyclic(|weak: &std::rc::Weak<Self>| {
            let weak = weak.clone();
            let languages = LanguagePicker::new(
                language,
                selected,
                Rc::new(move |language| {
                    if let Some(owner) = weak.upgrade() {
                        language_changed(language);
                        owner.refresh();
                        (owner.changed)(owner.selected());
                    }
                }),
            );
            widget.append(&languages.widget);
            let requirements = gtk::Label::builder().xalign(0.0).wrap(true).build();
            requirements.add_css_class("caption");
            widget.append(&requirements);
            let progress = gtk::ProgressBar::builder().show_text(true).build();
            widget.append(&progress);
            let button = gtk::Button::builder()
                .label("Download model")
                .halign(gtk::Align::Start)
                .build();
            widget.append(&button);
            let status = gtk::Label::builder().xalign(0.0).wrap(true).build();
            widget.append(&status);
            // sysconf is read-only and exposes no audio/device/user content.
            let ram = unsafe { libc::sysconf(libc::_SC_PHYS_PAGES) } as f64
                * unsafe { libc::sysconf(libc::_SC_PAGESIZE) } as f64
                / 1024_f64.powi(3);
            let machine = gtk::Label::builder().label(format!("This computer: {ram:.0} GiB RAM · no GPU required\nLocal previews use short audio chunks. Qwen also streams text as it decodes. No paid fallback.")).xalign(0.0).wrap(true).build();
            machine.add_css_class("caption");
            widget.append(&machine);
            Self {
                widget,
                label,
                scale,
                gpu,
                languages,
                requirements,
                progress,
                button,
                status,
                data,
                runtime: runtime.clone(),
                changed,
                updating: Cell::new(false),
                generation: Cell::new(0),
                cancellation: RefCell::new(None),
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.scale.connect_value_changed(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.selection_changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.gpu.connect_toggled(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.selection_changed();
            }
        });
        let weak = Rc::downgrade(&owner);
        owner.button.connect_clicked(move |_| {
            if let Some(owner) = weak.upgrade() {
                owner.start();
            }
        });
        owner.refresh();
        let probe = runtime.spawn_background(gpu_name());
        let weak = Rc::downgrade(&owner);
        runtime.spawn(async move {
            if let Ok(name) = probe.await
                && let Some(owner) = weak.upgrade()
            {
                owner
                    .gpu
                    .set_visible(!name.is_empty() || owner.device() == "cuda");
                owner.gpu.set_tooltip_text(Some(if name.is_empty() {
                    "NVIDIA GPU unavailable on this computer"
                } else {
                    name
                }));
            }
        });
        owner
    }
    pub fn selected(&self) -> &'static str {
        &MODEL_CATALOG
            .iter()
            .filter(|model| model.offered)
            .nth(self.scale.value().round_ties_even() as usize)
            .expect("bounded model slider")
            .id
    }
    pub fn device(&self) -> &'static str {
        if self.gpu.is_active() { "cuda" } else { "cpu" }
    }
    pub fn downloaded(&self) -> bool {
        let spec = model(self.selected()).expect("offered model");
        spec.ready(&self.data)
            && if spec.engine == "qwen3-asr" {
                QWEN_RUNTIME.ready(&self.data, self.device())
            } else {
                ONNX_RUNTIME.ready(&self.data, self.device())
            }
    }
    pub fn is_ready(&self) -> bool {
        self.downloaded() && languages::supports(self.selected(), &self.languages.language())
    }
    pub fn refresh(&self) {
        let spec = model(self.selected()).expect("offered model");
        self.languages
            .refresh(&self.languages.language(), self.selected());
        self.label.set_label(&format!(
            "{} · {} GB RAM · {:.0} MB storage",
            spec.label,
            spec.ram_mb as f64 / 1000.0,
            spec.bytes() as f64 / 1_000_000.0
        ));
        let extra = if self.device() == "cuda" {
            if self.selected() == "qwen3-1.7b" {
                "Compact GPU runtime included. "
            } else {
                "GPU support adds up to 3.5 GB storage and additional working RAM. "
            }
        } else {
            ""
        };
        self.requirements
            .set_label(&format!("{extra}Estimated working memory on CPU."));
        let available = self.downloaded();
        self.status.set_label(if available {
            "Ready to use. Model memory is released after recording."
        } else {
            "Download required before continuing."
        });
        if !languages::supports(self.selected(), &self.languages.language()) {
            self.status
                .set_label("Choose a supported language above before continuing.");
        }
        self.progress
            .set_fraction(if available { 1.0 } else { 0.0 });
        self.progress
            .set_text(Some(if available { "Ready" } else { "Not downloaded" }));
        self.progress.set_visible(false);
        self.button.set_sensitive(!available);
        self.button.set_visible(!available);
    }
    pub fn set_selection(&self, identifier: &str, device: &str) {
        self.updating.set(true);
        if let Some(index) = MODEL_CATALOG
            .iter()
            .filter(|model| model.offered)
            .position(|model| model.id == identifier)
        {
            self.scale.set_value(index as f64);
        }
        self.gpu.set_active(device == "cuda");
        self.updating.set(false);
        self.refresh();
    }
    fn selection_changed(&self) {
        if !self.updating.get() {
            self.stop();
            self.refresh();
            (self.changed)(self.selected());
        }
    }
    pub fn start(self: &Rc<Self>) {
        if self.downloaded() || self.cancellation.borrow().is_some() {
            return;
        }
        self.generation.set(self.generation.get() + 1);
        let generation = self.generation.get();
        let token = CancellationToken::new();
        self.cancellation.replace(Some(token.clone()));
        self.button.set_sensitive(false);
        self.progress.set_visible(true);
        self.status.set_label(if self.device() == "cuda" {
            "Installing GPU support and verifying model…"
        } else {
            "Downloading and verifying model…"
        });
        (self.changed)(self.selected());
        let identifier = self.selected();
        let device = self.device();
        let data = self.data.clone();
        let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
        let task = self.runtime.spawn_background(async move {
            let store = AssetStore::new(&data)?;
            let mut download = store.begin_download()?;
            let spec = model(identifier)?;
            if spec.engine == "qwen3-asr" {
                download
                    .install_qwen_runtime(&QWEN_RUNTIME, device, &token)
                    .await?;
            } else {
                download
                    .install_onnx_runtime(&ONNX_RUNTIME, device, &token)
                    .await?;
            }
            let mut previous = None;
            download
                .install_model_files(spec, &token, &mut move |value| {
                    let percent = (value * 100.0) as u32;
                    if previous.replace(percent) != Some(percent) {
                        let _ = sender.send(value);
                    }
                })
                .await
        });
        let weak = Rc::downgrade(self);
        self.runtime.spawn(async move {
            while let Some(value) = receiver.recv().await {
                if let Some(owner) = weak.upgrade()
                    && owner.generation.get() == generation
                {
                    owner.progress.set_fraction(value);
                    owner.progress.set_text(Some(&format!(
                        "{:.0}% · verifying before use",
                        value * 100.0
                    )));
                }
            }
            let message = match task.await {
                Ok(Ok(())) => String::new(),
                Ok(Err(error)) => error.to_string(),
                Err(_) => {
                    "Download did not finish. Check your connection and disk space, then retry."
                        .into()
                }
            };
            if let Some(owner) = weak.upgrade()
                && owner.generation.get() == generation
            {
                owner.cancellation.borrow_mut().take();
                owner.refresh();
                if !message.is_empty() {
                    owner.status.set_label(&message);
                }
                (owner.changed)(owner.selected());
            }
        });
    }
    pub fn stop(&self) {
        self.generation.set(self.generation.get() + 1);
        if let Some(token) = self.cancellation.borrow_mut().take() {
            token.cancel();
        }
    }
}
impl Drop for LocalModelSettings {
    fn drop(&mut self) {
        self.stop();
    }
}
