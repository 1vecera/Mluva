use std::cell::{Cell, RefCell};
use std::collections::{BTreeMap, BTreeSet};

use glib::subclass::prelude::*;
use gtk::pango;
use gtk::prelude::*;
use gtk::subclass::prelude::*;
use mluva_core::markdown::{
    MarkdownSpan, markdown_spans, needs_character_wrapping, visible_markdown,
};
use mluva_core::text_diff::text_changes;

const MAX_FORMATTING_SPANS: usize = 2_048;

mod imp {
    use super::*;

    #[derive(Default)]
    pub struct MarkdownTextView {
        pub markdown: Cell<bool>,
        pub document: Cell<bool>,
        pub replacing: Cell<bool>,
        pub formatting_limited: Cell<bool>,
        pub spans: RefCell<Vec<MarkdownSpan>>,
        pub styles: RefCell<BTreeMap<String, gtk::TextTag>>,
        pub diagram_source: RefCell<String>,
        pub diagram_ranges: RefCell<Vec<(usize, usize)>>,
    }

    #[glib::object_subclass]
    impl ObjectSubclass for MarkdownTextView {
        const NAME: &'static str = "MluvaMarkdownTextView";
        type Type = super::MarkdownTextView;
        type ParentType = gtk::TextView;
    }

    impl ObjectImpl for MarkdownTextView {
        fn constructed(&self) {
            self.parent_constructed();
            let obj = self.obj();
            obj.add_css_class("ml-transcript");
            obj.set_top_margin(4);
            obj.set_bottom_margin(12);
            obj.set_left_margin(2);
            obj.set_right_margin(6);
            let styles = [
                (
                    "syntax",
                    gtk::TextTag::builder()
                        .name("markdown-syntax")
                        .invisible(true)
                        .build(),
                ),
                (
                    "strong",
                    gtk::TextTag::builder()
                        .name("markdown-strong")
                        .weight(pango::Weight::Semibold.into_glib())
                        .build(),
                ),
                (
                    "em",
                    gtk::TextTag::builder()
                        .name("markdown-em")
                        .style(pango::Style::Italic)
                        .build(),
                ),
                (
                    "code",
                    gtk::TextTag::builder()
                        .name("markdown-code")
                        .family("JetBrains Mono")
                        .build(),
                ),
                (
                    "quote",
                    gtk::TextTag::builder()
                        .name("markdown-quote")
                        .style(pango::Style::Italic)
                        .build(),
                ),
                (
                    "list",
                    gtk::TextTag::builder().name("markdown-list").build(),
                ),
            ];
            let buffer = obj.buffer();
            for (name, tag) in styles {
                buffer.tag_table().add(&tag);
                self.styles.borrow_mut().insert(name.into(), tag);
            }
            for level in 1..=6 {
                let name = format!("h{level}");
                let tag = gtk::TextTag::builder()
                    .name(format!("markdown-{name}"))
                    .weight(pango::Weight::Semibold.into_glib())
                    .scale(super::heading_scale(level))
                    .build();
                buffer.tag_table().add(&tag);
                self.styles.borrow_mut().insert(name, tag);
            }
            buffer.connect_changed(glib::clone!(
                #[weak]
                obj,
                move |_| obj.format_document()
            ));
            obj.connect_editable_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.focus_changed()
            ));
            obj.connect_has_focus_notify(glib::clone!(
                #[weak]
                obj,
                move |_| obj.focus_changed()
            ));
        }
    }
    impl WidgetImpl for MarkdownTextView {
        fn request_mode(&self) -> gtk::SizeRequestMode {
            if self.document.get() {
                gtk::SizeRequestMode::HeightForWidth
            } else {
                self.parent_request_mode()
            }
        }
        fn measure(&self, orientation: gtk::Orientation, for_size: i32) -> (i32, i32, i32, i32) {
            if orientation == gtk::Orientation::Horizontal {
                (0, if self.document.get() { 300 } else { 0 }, -1, -1)
            } else if self.document.get() {
                let obj = self.obj();
                let height = obj.document_height(if for_size > 0 { for_size } else { 300 })
                    + obj.top_margin()
                    + obj.bottom_margin()
                    + 8;
                (height, height, -1, -1)
            } else {
                self.parent_measure(orientation, for_size)
            }
        }
    }
    impl TextViewImpl for MarkdownTextView {}
}

use glib::translate::IntoGlib;

glib::wrapper! {
    pub struct MarkdownTextView(ObjectSubclass<imp::MarkdownTextView>)
        @extends gtk::TextView, gtk::Widget,
        @implements gtk::Accessible, gtk::Buildable, gtk::ConstraintTarget, gtk::Scrollable;
}

pub fn heading_scale(level: usize) -> f64 {
    (1.14 - level as f64 * 0.02).max(1.04)
}

impl MarkdownTextView {
    /// Size the whole document inside one shared outer scrolling viewport.
    pub fn document(source: &str, markdown: bool, editable: bool) -> Self {
        let obj = Self::new(source, markdown, editable);
        obj.imp().document.set(true);
        obj.set_vexpand(false);
        obj.queue_resize();
        obj
    }

    pub fn is_markdown(&self) -> bool {
        self.imp().markdown.get()
    }

    pub fn new(source: &str, markdown: bool, editable: bool) -> Self {
        let obj: Self = glib::Object::builder()
            .property("editable", editable)
            .property("cursor-visible", editable)
            .property("wrap-mode", gtk::WrapMode::WordChar)
            .property("accepts-tab", false)
            .build();
        obj.imp().markdown.set(markdown);
        obj.buffer().set_text(source);
        obj.format_document();
        if markdown && editable {
            obj.set_tooltip_text(Some(
                "Click to edit the Markdown source. Copy and Save keep its formatting.",
            ));
        }
        obj
    }

    pub fn text(&self) -> String {
        let buffer = self.buffer();
        buffer
            .text(&buffer.start_iter(), &buffer.end_iter(), true)
            .into()
    }

    pub fn spans(&self) -> Vec<MarkdownSpan> {
        self.imp().spans.borrow().clone()
    }

    pub fn formatting_limited(&self) -> bool {
        self.imp().formatting_limited.get()
    }

    pub fn replace_text(&self, source: &str) {
        let changes = text_changes(&self.text(), source);
        if changes.is_empty() {
            return;
        }
        let source = source.chars().collect::<Vec<_>>();
        let buffer = self.buffer();
        self.imp().replacing.set(true);
        for change in changes.iter().rev() {
            let mut start = buffer.iter_at_offset(change.old_start as i32);
            let mut end = buffer.iter_at_offset(change.old_end as i32);
            buffer.delete(&mut start, &mut end);
            buffer.insert(
                &mut start,
                &source[change.new_start..change.new_end]
                    .iter()
                    .collect::<String>(),
            );
        }
        self.imp().replacing.set(false);
        self.format_document();
    }

    pub fn set_diagram_ranges(&self, source: &str, ranges: &[(usize, usize)]) {
        self.imp().diagram_source.replace(source.into());
        self.imp().diagram_ranges.replace(ranges.into());
        self.format_document();
    }

    fn focus_changed(&self) {
        self.imp().styles.borrow()["syntax"]
            .set_invisible(!(self.is_editable() && self.has_focus()));
        self.queue_resize();
    }

    fn format_document(&self) {
        let imp = self.imp();
        if imp.replacing.get() {
            return;
        }
        let source = self.text();
        let mut spans = if imp.markdown.get() {
            markdown_spans(&source)
        } else {
            Vec::new()
        };
        if source == *imp.diagram_source.borrow() {
            spans.extend(
                imp.diagram_ranges
                    .borrow()
                    .iter()
                    .map(|&(start, end)| MarkdownSpan {
                        start,
                        end,
                        style: "syntax".into(),
                    }),
            );
        }
        imp.formatting_limited
            .set(spans.len() > MAX_FORMATTING_SPANS);
        if imp.formatting_limited.get() {
            spans.clear();
        }
        self.set_wrap_mode(if needs_character_wrapping(&source) {
            gtk::WrapMode::Char
        } else {
            gtk::WrapMode::WordChar
        });
        let buffer = self.buffer();
        let (start, end) = buffer.bounds();
        for tag in imp.styles.borrow().values() {
            buffer.remove_tag(tag, &start, &end);
        }
        let positions = spans
            .iter()
            .flat_map(|span| [span.start, span.end])
            .collect::<BTreeSet<_>>();
        let mut cursor = buffer.start_iter();
        let mut previous = 0;
        let mut endpoints = BTreeMap::new();
        for position in positions {
            cursor.forward_chars((position - previous) as i32);
            endpoints.insert(position, cursor);
            previous = position;
        }
        for span in &spans {
            if span.start < span.end {
                buffer.apply_tag(
                    &imp.styles.borrow()[&span.style],
                    &endpoints[&span.start],
                    &endpoints[&span.end],
                );
            }
        }
        imp.spans.replace(spans);
        if imp.markdown.get() && self.is_editable() {
            self.set_tooltip_text(Some(if imp.formatting_limited.get() {
                "This heavily formatted document is shown as plain Markdown. Copy and Save keep its complete source."
            } else {"Click to edit the Markdown source. Copy and Save keep its formatting."}));
        }
        self.queue_resize();
    }

    pub fn document_height(&self, width: i32) -> i32 {
        let source = self.text();
        let spans = self.spans();
        let (source, mut spans) = if self.imp().styles.borrow()["syntax"].is_invisible() {
            visible_markdown(&source, &spans)
        } else {
            (source, spans)
        };
        spans.retain(|span| span.style != "syntax");
        spans.sort_by_key(|span| span.start);
        let source = source.chars().collect::<Vec<_>>();
        let mut boundaries = Vec::new();
        let mut index = 0;
        while index < source.len() {
            if matches!(source[index], '\r' | '\n' | '\u{2029}') {
                let end = index;
                if source[index] == '\r' && source.get(index + 1) == Some(&'\n') {
                    index += 1;
                }
                boundaries.push((end, index + 1));
            }
            index += 1;
        }
        boundaries.push((source.len(), source.len()));
        let (mut offset, mut style_index, mut height) = (0, 0, 0);
        let mut cache = BTreeMap::new();
        for (end, next) in boundaries {
            let paragraph = source[offset..end].iter().collect::<String>();
            let first = style_index;
            while style_index < spans.len() && spans[style_index].start < end {
                style_index += 1;
            }
            let local = spans[first..style_index]
                .iter()
                .map(|span| MarkdownSpan {
                    start: span.start.saturating_sub(offset),
                    end: span.end.saturating_sub(offset),
                    style: span.style.clone(),
                })
                .collect::<Vec<_>>();
            let key = (
                paragraph.clone(),
                local
                    .iter()
                    .map(|span| (span.start, span.end, span.style.clone()))
                    .collect::<Vec<_>>(),
            );
            let measured = cache.entry(key).or_insert_with(|| {
                (f64::from(self.paragraph_layout(&paragraph, &local, width).size().1)
                    / f64::from(pango::SCALE))
                .round_ties_even() as i32
            });
            height += *measured;
            offset = next;
        }
        height
    }

    fn paragraph_layout(&self, source: &str, spans: &[MarkdownSpan], width: i32) -> pango::Layout {
        let layout = self.create_pango_layout(Some(if source.is_empty() { " " } else { source }));
        layout.set_width((width - self.left_margin() - self.right_margin()).max(1) * pango::SCALE);
        layout.set_wrap(if self.wrap_mode() == gtk::WrapMode::Char {
            pango::WrapMode::Char
        } else {
            pango::WrapMode::WordChar
        });
        let attributes = pango::AttrList::new();
        attributes.insert(pango::AttrFloat::new_line_height(1.3));
        let offsets = source
            .char_indices()
            .map(|(byte, _)| byte)
            .chain([source.len()])
            .collect::<Vec<_>>();
        for span in spans {
            let mut attrs = Vec::<pango::Attribute>::new();
            if span.style == "strong" || span.style.starts_with('h') {
                attrs.push(pango::AttrInt::new_weight(pango::Weight::Semibold).into());
            }
            if let Some(level) = span
                .style
                .strip_prefix('h')
                .and_then(|level| level.parse().ok())
            {
                attrs.push(pango::AttrFloat::new_scale(heading_scale(level)).into());
            }
            if matches!(span.style.as_str(), "em" | "quote") {
                attrs.push(pango::AttrInt::new_style(pango::Style::Italic).into());
            }
            if span.style == "code" {
                attrs.push(pango::AttrString::new_family("JetBrains Mono").into());
            }
            for mut attr in attrs {
                attr.set_start_index(offsets[span.start.min(offsets.len() - 1)] as u32);
                attr.set_end_index(offsets[span.end.min(offsets.len() - 1)] as u32);
                attributes.insert(attr);
            }
        }
        layout.set_attributes(Some(&attributes));
        layout
    }
}
