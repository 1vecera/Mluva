//! Offline, ephemeral Mermaid rendering with lossless native source editing.

use crate::{document_layout::DocumentResources, markdown_view::MarkdownTextView};
use adw::prelude::*;
use base64::{Engine, engine::general_purpose::STANDARD};
use regex::Regex;
use serde::Deserialize;
use std::cell::{Cell, RefCell};
use std::rc::Rc;
use std::sync::LazyLock;
use webkit6::prelude::*;

pub const MAX_DIAGRAMS: usize = 3;
pub const MAX_DIAGRAM_CHARACTERS: usize = 12_000;

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
pub struct MermaidBlock {
    pub start: usize,
    pub end: usize,
    pub text: String,
}

/// Character offsets refer to the original Markdown, including its fence newlines.
pub fn mermaid_blocks(source: &str) -> Vec<MermaidBlock> {
    static FENCE: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"^ {0,3}(`{3,}|~{3,})([^\r\n]*)$").unwrap());
    let mut blocks = Vec::new();
    let mut fence = String::new();
    let mut language = String::new();
    let (mut start, mut body, mut offset) = (0, 0, 0);
    let characters = source.chars().collect::<Vec<_>>();
    let mut beginning = 0;
    while beginning < characters.len() {
        let mut ending = beginning;
        while ending < characters.len()
            && !matches!(
                characters[ending],
                '\n' | '\r' | '\u{b}' | '\u{c}' | '\u{1c}'
                    ..='\u{1e}' | '\u{85}' | '\u{2028}' | '\u{2029}'
            )
        {
            ending += 1;
        }
        if ending < characters.len() {
            ending += 1;
            if characters[ending - 1] == '\r' && characters.get(ending) == Some(&'\n') {
                ending += 1;
            }
        }
        let line = characters[beginning..ending].iter().collect::<String>();
        if let Some(m) = FENCE.captures(line.trim_end_matches(['\r', '\n'])) {
            if fence.is_empty() {
                fence = m[1].into();
                language = m[2].into();
                start = offset;
                body = offset + ending - beginning;
            } else if m[1].starts_with(fence.chars().next().unwrap())
                && m[1].len() >= fence.len()
                && mluva_core::text::trim(&m[2]).is_empty()
            {
                if caseless::default_case_fold_str(mluva_core::text::trim(&language)) == "mermaid"
                    && offset > body
                    && offset - body <= MAX_DIAGRAM_CHARACTERS
                {
                    blocks.push(MermaidBlock {
                        start,
                        end: offset + ending - beginning,
                        text: characters[body..offset].iter().collect(),
                    });
                    if blocks.len() == MAX_DIAGRAMS {
                        break;
                    }
                }
                fence.clear();
                language.clear();
            }
        }
        offset += ending - beginning;
        beginning = ending;
    }
    blocks
}

pub fn native_svg(svg: &str) -> Result<Vec<u8>, String> {
    if svg.chars().count() > 1_000_000 {
        return Err("Sketch too large".into());
    }
    // Resolve local XML entities without any external entity resolver or network access.
    let document = roxmltree::Document::parse_with_options(
        svg,
        roxmltree::ParsingOptions {
            allow_dtd: true,
            ..Default::default()
        },
    )
    .map_err(|_| "Invalid sketch XML")?;
    for element in document.descendants().filter(|node| node.is_element()) {
        if matches!(
            element.tag_name().name(),
            "foreignObject" | "script" | "image"
        ) {
            return Err("Unsupported sketch content".into());
        }
        for attribute in element.attributes() {
            if attribute.name() == "href" && !attribute.value().starts_with('#') {
                return Err("External sketch reference".into());
            }
        }
    }
    static RESOURCES: LazyLock<Regex> =
        LazyLock::new(|| Regex::new(r"(?is)url\s*\((.*?)\)").unwrap());
    for reference in RESOURCES.captures_iter(svg) {
        if !reference[1]
            .trim_matches(mluva_core::text::whitespace)
            .trim_matches(['\'', '"'])
            .starts_with('#')
        {
            return Err("External sketch resource".into());
        }
    }
    Ok(svg.as_bytes().into())
}

#[derive(Deserialize)]
struct Rendered {
    revision: u64,
    images: Vec<RenderedImage>,
}
#[derive(Deserialize)]
struct RenderedImage {
    index: usize,
    svg: String,
}

#[derive(Default)]
struct RenderState {
    web: Option<webkit6::WebView>,
    loaded: bool,
    in_flight: bool,
    revision: u64,
    source: String,
    blocks: Vec<MermaidBlock>,
    pictures: Vec<gtk::Picture>,
    rendered_codes: Vec<String>,
    rendered_indices: Vec<usize>,
    rendered_dark: Option<bool>,
    rendering_dark: Option<bool>,
}

pub struct MermaidPreview {
    pub widget: gtk::Box,
    pub notice: gtk::Label,
    editor: MarkdownTextView,
    resources: DocumentResources,
    state: RefCell<RenderState>,
    pending: RefCell<Option<glib::SourceId>>,
    watchdog: RefCell<Option<glib::SourceId>>,
    theme_handler: RefCell<Option<glib::SignalHandlerId>>,
    changing: Cell<bool>,
}

impl MermaidPreview {
    pub fn new(editor: &MarkdownTextView, resources: &DocumentResources) -> Rc<Self> {
        let widget = gtk::Box::new(gtk::Orientation::Vertical, 4);
        let notice = gtk::Label::builder()
            .xalign(0.0)
            .wrap(true)
            .css_classes(["caption", "dim-label"])
            .build();
        widget.append(&notice);
        let preview = Rc::new(Self {
            widget,
            notice,
            editor: editor.clone(),
            resources: resources.clone(),
            state: RefCell::new(RenderState::default()),
            pending: RefCell::new(None),
            watchdog: RefCell::new(None),
            theme_handler: RefCell::new(None),
            changing: Cell::new(false),
        });
        let weak = Rc::downgrade(&preview);
        editor.buffer().connect_changed(move |_| {
            if let Some(p) = weak.upgrade() {
                p.changed();
            }
        });
        let weak = Rc::downgrade(&preview);
        preview.widget.connect_map(move |_| {
            if let Some(p) = weak.upgrade() {
                p.changed();
            }
        });
        let weak = Rc::downgrade(&preview);
        preview.widget.connect_unmap(move |_| {
            if let Some(p) = weak.upgrade() {
                p.clear_pending();
                let mut s = p.state.borrow_mut();
                s.revision += 1;
                s.in_flight = false;
                drop(s);
                p.clear_watchdog();
            }
        });
        let weak = Rc::downgrade(&preview);
        preview
            .theme_handler
            .replace(Some(adw::StyleManager::default().connect_dark_notify(
                move |_| {
                    if let Some(p) = weak.upgrade() {
                        p.changed();
                    }
                },
            )));
        preview.changed();
        preview
    }

    fn clear_pending(&self) {
        if let Some(id) = self.pending.borrow_mut().take() {
            id.remove();
        }
    }
    fn clear_watchdog(&self) {
        if let Some(id) = self.watchdog.borrow_mut().take() {
            id.remove();
        }
    }

    fn changed(self: &Rc<Self>) {
        if self.changing.replace(true) {
            return;
        }
        let source = self.editor.text();
        let blocks = if self.editor.is_markdown() {
            mermaid_blocks(&source)
        } else {
            Vec::new()
        };
        self.clear_pending();
        let s = self.state.borrow();
        let reused = !blocks.is_empty()
            && blocks.iter().map(|b| &b.text).eq(s.rendered_codes.iter())
            && s.rendered_dark == Some(adw::StyleManager::default().is_dark())
            && !s.pictures.is_empty();
        if reused {
            let ranges = s
                .rendered_indices
                .iter()
                .filter_map(|&i| blocks.get(i))
                .map(|b| (b.start, b.end))
                .collect::<Vec<_>>();
            self.editor.set_diagram_ranges(&source, &ranges);
            for picture in &s.pictures {
                picture.set_visible(true);
            }
        } else {
            for picture in &s.pictures {
                picture.set_visible(false);
            }
        }
        drop(s);
        self.widget.set_visible(!blocks.is_empty());
        self.state.borrow_mut().blocks = blocks;
        if !reused && !self.state.borrow().blocks.is_empty() && self.widget.is_mapped() {
            let weak = Rc::downgrade(self);
            self.pending.replace(Some(glib::timeout_add_local_once(
                std::time::Duration::from_millis(450),
                move || {
                    if let Some(p) = weak.upgrade() {
                        p.pending.borrow_mut().take();
                        p.render();
                    }
                },
            )));
        }
        self.changing.set(false);
    }

    fn render(self: &Rc<Self>) {
        {
            let s = self.state.borrow();
            if s.in_flight || !self.widget.is_mapped() || s.blocks.is_empty() {
                return;
            }
        }
        if !self.resources.mermaid.join("mermaid.min.js").is_file() {
            self.notice.set_label(
                "Mermaid preview needs WebKitGTK 6.0. The editable sketch source is kept above.",
            );
            self.notice.set_visible(true);
            return;
        }
        if self.state.borrow().web.is_none() {
            let manager = webkit6::UserContentManager::new();
            manager.register_script_message_handler("rendered", None);
            let weak = Rc::downgrade(self);
            manager.connect_script_message_received(Some("rendered"), move |_, value| {
                if let Some(p) = weak.upgrade() {
                    p.rendered(value.to_string().as_str());
                }
            });
            let web = webkit6::WebView::builder()
                .network_session(&webkit6::NetworkSession::new_ephemeral())
                .user_content_manager(&manager)
                .build();
            web.set_background_color(&gtk::gdk::RGBA::new(0.0, 0.0, 0.0, 0.0));
            web.set_size_request(-1, 1);
            web.set_opacity(0.0);
            web.set_can_target(false);
            web.set_focusable(false);
            if let Some(settings) = webkit6::prelude::WebViewExt::settings(&web) {
                settings.set_enable_html5_local_storage(false);
                settings.set_enable_page_cache(false);
            }
            let base = gio::File::for_path(&self.resources.mermaid)
                .uri()
                .to_string()
                + "/";
            let allowed = base.clone();
            web.connect_decide_policy(move |_, decision, kind| {
                if matches!(
                    kind,
                    webkit6::PolicyDecisionType::NavigationAction
                        | webkit6::PolicyDecisionType::NewWindowAction
                ) {
                    let uri = decision
                        .downcast_ref::<webkit6::NavigationPolicyDecision>()
                        .and_then(|d| d.navigation_action())
                        .and_then(|mut a| a.request())
                        .and_then(|r| r.uri());
                    if !uri
                        .as_ref()
                        .is_some_and(|u| u == "about:blank" || u.as_str() == allowed)
                    {
                        decision.ignore();
                        return true;
                    }
                }
                false
            });
            let weak = Rc::downgrade(self);
            web.connect_load_changed(move |_, event| {
                if event == webkit6::LoadEvent::Finished
                    && let Some(p) = weak.upgrade()
                {
                    p.state.borrow_mut().loaded = true;
                    p.render();
                }
            });
            let weak = Rc::downgrade(self);
            web.connect_web_process_terminated(move |_, _| {
                if let Some(p) = weak.upgrade() {
                    p.failed();
                }
            });
            self.state.borrow_mut().web = Some(web.clone());
            self.widget.append(&web);
            let Ok(font) = std::fs::read(&self.resources.font) else {
                self.failed();
                return;
            };
            let html = include_str!("../resources/mermaid/preview.html")
                .replace("__FONT_DATA__", &STANDARD.encode(font));
            web.load_html(&html, Some(&base));
        }
        let mut s = self.state.borrow_mut();
        if !s.loaded {
            return;
        }
        s.in_flight = true;
        s.revision += 1;
        s.source = self.editor.text();
        s.blocks = mermaid_blocks(&s.source);
        let dark = adw::StyleManager::default().is_dark();
        s.rendering_dark = Some(dark);
        let color = self.editor.color();
        let ink = format!(
            "#{:02x}{:02x}{:02x}",
            (f64::from(color.red()) * 255.0).round_ties_even() as u8,
            (f64::from(color.green()) * 255.0).round_ties_even() as u8,
            (f64::from(color.blue()) * 255.0).round_ties_even() as u8
        );
        let payload = serde_json::json!([
            s.revision,
            s.blocks.iter().map(|b| &b.text).collect::<Vec<_>>(),
            dark,
            ink,
            ink
        ]);
        let web = s.web.clone().unwrap();
        drop(s);
        web.evaluate_javascript(
            &format!("draw(...{payload})"),
            None,
            None,
            gio::Cancellable::NONE,
            |_| {},
        );
        self.clear_watchdog();
        let weak = Rc::downgrade(self);
        self.watchdog.replace(Some(glib::timeout_add_local_once(
            std::time::Duration::from_secs(8),
            move || {
                if let Some(p) = weak.upgrade() {
                    p.watchdog.borrow_mut().take();
                    let web = p.state.borrow().web.clone();
                    if let Some(web) = web {
                        web.terminate_web_process();
                    }
                }
            },
        )));
    }

    fn rendered(self: &Rc<Self>, message: &str) {
        let Ok(result) = serde_json::from_str::<Rendered>(message) else {
            return;
        };
        let mut s = self.state.borrow_mut();
        if result.revision != s.revision {
            return;
        }
        s.in_flight = false;
        self.clear_watchdog();
        if s.source != self.editor.text()
            || s.rendering_dark != Some(adw::StyleManager::default().is_dark())
        {
            drop(s);
            self.changed();
            return;
        }
        for picture in s.pictures.drain(..) {
            self.widget.remove(&picture);
        }
        let mut rendered = Vec::new();
        for item in result.images {
            if item.index >= s.blocks.len() {
                continue;
            }
            let Ok(svg) = native_svg(&item.svg) else {
                continue;
            };
            let Ok(texture) = gtk::gdk::Texture::from_bytes(&glib::Bytes::from_owned(svg)) else {
                continue;
            };
            let picture = gtk::Picture::for_paintable(&texture);
            picture.set_can_shrink(true);
            picture.set_vexpand(false);
            picture.set_content_fit(gtk::ContentFit::Contain);
            picture.update_property(&[gtk::accessible::Property::Label(
                "Mermaid sketch; editable source above",
            )]);
            self.widget.append(&picture);
            s.pictures.push(picture);
            rendered.push(item.index);
        }
        self.editor.set_diagram_ranges(
            &s.source,
            &rendered
                .iter()
                .map(|&i| (s.blocks[i].start, s.blocks[i].end))
                .collect::<Vec<_>>(),
        );
        s.rendered_codes = s.blocks.iter().map(|b| b.text.clone()).collect();
        s.rendered_indices = rendered;
        s.rendered_dark = s.rendering_dark;
        self.notice
            .set_label(if s.rendered_indices.len() == s.blocks.len() {
                ""
            } else {
                "Incomplete sketch — source kept above."
            });
        self.notice.set_visible(!self.notice.label().is_empty());
    }

    fn failed(&self) {
        self.clear_watchdog();
        let mut s = self.state.borrow_mut();
        s.loaded = false;
        s.in_flight = false;
        let web = s.web.take();
        drop(s);
        self.editor.set_diagram_ranges(&self.editor.text(), &[]);
        self.notice
            .set_label("Sketch preview unavailable. The editable Mermaid source is kept above.");
        self.notice.set_visible(true);
        if let Some(web) = web {
            self.widget.remove(&web);
        }
    }
}

impl Drop for MermaidPreview {
    fn drop(&mut self) {
        self.clear_pending();
        self.clear_watchdog();
        if let Some(handler) = self.theme_handler.get_mut().take() {
            adw::StyleManager::default().disconnect(handler);
        }
        if let Some(web) = self.state.get_mut().web.take() {
            web.terminate_web_process();
        }
    }
}
