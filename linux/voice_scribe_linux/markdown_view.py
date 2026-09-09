"""Display subtle native Markdown while keeping editing and explicit copying lossless."""

import re

import gi

from voice_scribe_linux.minimal_markdown import MarkdownSpan, markdown_spans, needs_character_wrapping, visible_markdown

gi.require_version("Gtk", "4.0")
from gi.repository import Gtk, Pango  # noqa: E402

MAX_FORMATTING_SPANS = 2048
# U+2028 is a soft line break inside one native paragraph. Keeping it in the
# layout preserves Pango's fractional line-height accumulation for long notes.
_PARAGRAPH_ENDING = re.compile(r"\r\n|[\r\n\u2029]")


class MarkdownTextView(Gtk.TextView):
    """Keep the actual Markdown in one buffer, revealing its syntax while editing."""

    def __init__(self, text: str = "", *, markdown: bool = True, editable: bool = True) -> None:
        """Use theme fonts and modest emphasis without HTML, embedded media or external links."""
        super().__init__(
            editable=editable,
            cursor_visible=editable,
            wrap_mode=Gtk.WrapMode.WORD_CHAR,
            accepts_tab=False,
        )
        self.markdown = markdown
        self.spans = ()
        self.formatting_limited = False
        self.add_css_class("ml-transcript")
        self.set_top_margin(4)
        self.set_bottom_margin(12)
        self.set_left_margin(2)
        self.set_right_margin(6)
        buffer = self.get_buffer()
        self.styles = {
            "syntax": buffer.create_tag("markdown-syntax", invisible=True),
            "strong": buffer.create_tag("markdown-strong", weight=Pango.Weight.SEMIBOLD),
            "em": buffer.create_tag("markdown-em", style=Pango.Style.ITALIC),
            "code": buffer.create_tag("markdown-code", family="monospace"),
            "quote": buffer.create_tag("markdown-quote", style=Pango.Style.ITALIC),
            "list": buffer.create_tag("markdown-list"),
        }
        for level in range(1, 7):
            self.styles[f"h{level}"] = buffer.create_tag(
                f"markdown-h{level}", weight=Pango.Weight.SEMIBOLD, scale=self.heading_scale(level)
            )
        buffer.connect("changed", self._format)
        buffer.set_text(text)
        self.connect("notify::editable", self._focus_changed)
        self.connect("notify::has-focus", self._focus_changed)
        if markdown and editable:
            self.set_tooltip_text("Click to edit the Markdown source. Copy and Save keep its formatting.")

    @staticmethod
    def heading_scale(level: int) -> float:
        """Keep every heading near body size, including model-generated top-level titles."""
        return max(1.04, 1.14 - level * 0.02)

    def get_text(self) -> str:
        """Return the exact source, including hidden Markdown delimiters and whitespace."""
        buffer = self.get_buffer()
        return buffer.get_text(buffer.get_start_iter(), buffer.get_end_iter(), True)

    def replace_text(self, text: str) -> None:
        """Change only the differing range so unaffected selections and caret marks survive."""
        previous = self.get_text()
        if previous == text:
            return
        shared = 0
        for before, after in zip(previous, text, strict=False):
            if before != after:
                break
            shared += 1
        suffix = 0
        limit = min(len(previous), len(text)) - shared
        while suffix < limit and previous[-suffix - 1] == text[-suffix - 1]:
            suffix += 1
        buffer = self.get_buffer()
        buffer.delete(buffer.get_iter_at_offset(shared), buffer.get_iter_at_offset(len(previous) - suffix))
        buffer.insert(buffer.get_iter_at_offset(shared), text[shared : len(text) - suffix])

    def _focus_changed(self, *_args: object) -> None:
        """Make source syntax discoverable for edits without changing the document or its revision."""
        self.styles["syntax"].set_property("invisible", not (self.get_editable() and self.has_focus()))
        self.queue_resize()

    def _format(self, buffer: Gtk.TextBuffer) -> None:
        """Bound native tag work; unusually dense formatting remains complete plain Markdown."""
        source = self.get_text()
        spans = markdown_spans(source) if self.markdown else ()
        self.formatting_limited = len(spans) > MAX_FORMATTING_SPANS
        self.spans = () if self.formatting_limited else spans
        self.set_wrap_mode(Gtk.WrapMode.CHAR if needs_character_wrapping(source) else Gtk.WrapMode.WORD_CHAR)
        start, end = buffer.get_bounds()
        for tag in self.styles.values():
            buffer.remove_tag(tag, start, end)
        # Walking one iterator forward avoids repeatedly decoding a long UTF-8
        # paragraph from its start for each tag endpoint.
        positions = sorted({position for span in self.spans for position in (span.start, span.end)})
        cursor = buffer.get_start_iter()
        previous = 0
        endpoints = {}
        for position in positions:
            cursor.forward_chars(position - previous)
            endpoints[position] = cursor.copy()
            previous = position
        for span in self.spans:
            if span.start < span.end:
                buffer.apply_tag(self.styles[span.style], endpoints[span.start], endpoints[span.end])
        if self.markdown and self.get_editable():
            self.set_tooltip_text(
                "This heavily formatted document is shown as plain Markdown. Copy and Save keep its complete source."
                if self.formatting_limited
                else "Click to edit the Markdown source. Copy and Save keep its formatting."
            )
        self.queue_resize()

    def document_height(self, width: int) -> int:
        """Measure paragraphs separately, avoiding Pango's repeated whole-document scans."""
        text, spans = self.get_text(), self.spans
        if self.styles["syntax"].get_property("invisible"):
            text, spans = visible_markdown(text, spans)
        styles = sorted((span for span in spans if span.style != "syntax"), key=lambda span: span.start)
        heights = {}
        offset = style_index = height = 0
        boundaries = [(match.start(), match.end()) for match in _PARAGRAPH_ENDING.finditer(text)]
        boundaries.append((len(text), len(text)))
        for end, next_offset in boundaries:
            paragraph = text[offset:end]
            first_style = style_index
            while style_index < len(styles) and styles[style_index].start < end:
                style_index += 1
            local_styles = tuple(
                MarkdownSpan(span.start - offset, span.end - offset, span.style)
                for span in styles[first_style:style_index]
            )
            key = (paragraph, local_styles)
            if key not in heights:
                logical_height = self._paragraph_layout(paragraph, local_styles, width).get_size()[1]
                heights[key] = round(logical_height / Pango.SCALE)
            height += heights[key]
            offset = next_offset
        return height

    def _paragraph_layout(self, text: str, spans: tuple[MarkdownSpan, ...], width: int) -> Pango.Layout:
        """Match the native view's font emphasis and wrapping for one source paragraph."""
        layout = self.create_pango_layout(text or " ")
        inset = self.get_left_margin() + self.get_right_margin()
        layout.set_width(max(1, width - inset) * Pango.SCALE)
        layout.set_wrap(Pango.WrapMode.CHAR if self.get_wrap_mode() == Gtk.WrapMode.CHAR else Pango.WrapMode.WORD_CHAR)
        attributes = Pango.AttrList()
        attributes.insert(Pango.attr_line_height_new(1.3))
        byte_offsets = [0]
        for character in text:
            byte_offsets.append(byte_offsets[-1] + len(character.encode("utf-8")))
        for span in spans:
            attrs = []
            if span.style == "strong" or span.style.startswith("h"):
                attrs.append(Pango.attr_weight_new(Pango.Weight.SEMIBOLD))
            if span.style.startswith("h"):
                attrs.append(Pango.attr_scale_new(self.heading_scale(int(span.style[1:]))))
            elif span.style in {"em", "quote"}:
                attrs.append(Pango.attr_style_new(Pango.Style.ITALIC))
            elif span.style == "code":
                attrs.append(Pango.attr_family_new("monospace"))
            for attribute in attrs:
                attribute.start_index = byte_offsets[span.start]
                attribute.end_index = byte_offsets[span.end]
                attributes.insert(attribute)
        layout.set_attributes(attributes)
        return layout

    def do_measure(self, orientation, for_size):
        """Prevent dictated words or changing Markdown syntax from setting a live pane's width."""
        if orientation == Gtk.Orientation.HORIZONTAL:
            return (0, 0, -1, -1)
        return Gtk.TextView.do_measure(self, orientation, for_size)
