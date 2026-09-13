"""Display subtle native Markdown while keeping editing and explicit copying lossless."""

import re

import gi

from mluva_linux.minimal_markdown import MarkdownSpan, markdown_spans, needs_character_wrapping, visible_markdown
from mluva_linux.text_diff import text_changes

gi.require_version("Gtk", "4.0")
gi.require_version("Graphene", "1.0")
from gi.repository import Gdk, Graphene, Gtk, Pango  # noqa: E402

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
        self.diagram_source = ""
        self.diagram_ranges = ()
        self.formatting_limited = False
        self.add_css_class("ml-transcript")
        self.set_top_margin(4)
        self.set_bottom_margin(12)
        self.set_left_margin(2)
        self.set_right_margin(6)
        buffer = self.get_buffer()
        self._replacing = False
        self._revision_tick = 0
        self._revision_started = 0
        self._revision_progress = 1.0
        self._removed_runs = []
        self._revision_tag = buffer.create_tag("revision-arrival")
        self.styles = {
            "syntax": buffer.create_tag("markdown-syntax", invisible=True),
            "strong": buffer.create_tag("markdown-strong", weight=Pango.Weight.SEMIBOLD),
            "em": buffer.create_tag("markdown-em", style=Pango.Style.ITALIC),
            "code": buffer.create_tag("markdown-code", family="JetBrains Mono"),
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
        self.connect("unmap", lambda _widget: self._finish_revision())
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

    def replace_text(self, text: str, *, animate: bool = False) -> None:
        """Commit exact source immediately; animate only the changed words as a visual overlay."""
        previous = self.get_text()
        changes = text_changes(previous, text)
        if not changes:
            return
        self._finish_revision()
        buffer = self.get_buffer()
        animate = animate and self.get_mapped() and self.get_settings().get_property("gtk-enable-animations")
        if animate:
            visible = self.get_visible_rect()
            _, first_visible = self.get_iter_at_location(visible.x, visible.y)
            for change in changes[:32]:
                cursor = buffer.get_iter_at_offset(max(change.old_start, first_visible.get_offset()))
                while cursor.get_offset() < change.old_end and len(self._removed_runs) < 64:
                    end = cursor.copy()
                    self.forward_display_line_end(end)
                    stop = min(change.old_end, max(cursor.get_offset() + 1, end.get_offset()))
                    location = self.get_iter_location(cursor)
                    x, y = self.buffer_to_window_coords(Gtk.TextWindowType.WIDGET, location.x, location.y)
                    if y >= self.get_height():
                        break
                    if y + location.height >= 0 and y < self.get_height():
                        layout = self.create_pango_layout(previous[cursor.get_offset() : stop])
                        self._removed_runs.append((layout, x, y))
                    cursor = buffer.get_iter_at_offset(stop)
        self._replacing = True
        for change in reversed(changes):
            buffer.delete(buffer.get_iter_at_offset(change.old_start), buffer.get_iter_at_offset(change.old_end))
            buffer.insert(buffer.get_iter_at_offset(change.old_start), text[change.new_start : change.new_end])
        self._replacing = False
        if animate:
            for change in changes:
                buffer.apply_tag(
                    self._revision_tag,
                    buffer.get_iter_at_offset(change.new_start),
                    buffer.get_iter_at_offset(change.new_end),
                )
            self._revision_started = 0
            self._revision_progress = 0.0
            self._revision_color(0)
            self._revision_tick = self.add_tick_callback(self._animate_revision)

    def _revision_color(self, alpha: float) -> None:
        color = self.get_color()
        color.alpha = alpha
        self._revision_tag.set_property("foreground-rgba", color)

    def _animate_revision(self, _widget: Gtk.Widget, clock: Gdk.FrameClock) -> bool:
        if not self._revision_started:
            self._revision_started = clock.get_frame_time()
        self._revision_progress = (clock.get_frame_time() - self._revision_started) / 340_000
        self._revision_color(min(1, max(0, (self._revision_progress * 340 - 120) / 220)))
        self.queue_draw()
        if self._revision_progress >= 1:
            self._revision_tick = 0
            self._finish_revision()
            return False
        return True

    def _finish_revision(self) -> None:
        if self._revision_tick:
            self.remove_tick_callback(self._revision_tick)
            self._revision_tick = 0
        self._removed_runs.clear()
        self._revision_progress = 1.0
        self.get_buffer().remove_tag(self._revision_tag, *self.get_buffer().get_bounds())
        self.queue_draw()

    def do_snapshot(self, snapshot: Gtk.Snapshot) -> None:
        """Fade departing words above the already authoritative, lossless native text buffer."""
        Gtk.TextView.do_snapshot(self, snapshot)
        alpha = max(0, 1 - self._revision_progress * 340 / 120)
        if alpha and self._removed_runs:
            color = self.get_color()
            color.alpha = alpha
            for layout, x, y in self._removed_runs:
                snapshot.save()
                snapshot.translate(Graphene.Point().init(x, y))
                snapshot.append_layout(layout, color)
                snapshot.restore()

    def _focus_changed(self, *_args: object) -> None:
        """Make source syntax discoverable for edits without changing the document or its revision."""
        self._finish_revision()
        self.styles["syntax"].set_property("invisible", not (self.get_editable() and self.has_focus()))
        self.queue_resize()

    def _format(self, buffer: Gtk.TextBuffer) -> None:
        """Bound native tag work; unusually dense formatting remains complete plain Markdown."""
        if not self._replacing:
            self._finish_revision()
        source = self.get_text()
        spans = markdown_spans(source) if self.markdown else ()
        if source == self.diagram_source:
            spans += tuple(MarkdownSpan(start, end, "syntax") for start, end in self.diagram_ranges)
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

    def set_diagram_ranges(self, source: str, ranges: list[tuple[int, int]]) -> None:
        """Hide only successfully rendered sketch source at rest; editing and copying keep it intact."""
        self.diagram_source = source
        self.diagram_ranges = tuple(ranges)
        self._format(self.get_buffer())

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
                attrs.append(Pango.attr_family_new("JetBrains Mono"))
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
