"""Render bounded Mermaid sketches locally while retaining lossless Markdown editing."""

import json
import re
import xml.etree.ElementTree as ET
from pathlib import Path

import gi

from mluva_linux.markdown_view import MarkdownTextView

gi.require_version("Gtk", "4.0")
gi.require_version("Adw", "1")
try:
    gi.require_version("WebKit", "6.0")
    from gi.repository import WebKit
except (ValueError, ImportError):
    WebKit = None
from gi.repository import Adw, Gdk, GLib, Gtk  # noqa: E402

_FENCE = re.compile(r"^ {0,3}(`{3,}|~{3,})([^\r\n]*)$")
MAX_DIAGRAMS = 3
MAX_DIAGRAM_CHARACTERS = 12_000


def native_svg(svg: str) -> bytes:
    """Keep browser policy intact when handing a diagram to the native SVG decoder."""
    if len(svg) > 1_000_000:
        raise ValueError("Sketch too large")
    root = ET.fromstring(svg)
    for element in root.iter():
        if element.tag.rsplit("}", 1)[-1] in {"foreignObject", "script", "image"}:
            raise ValueError("Unsupported sketch content")
        for name, value in element.attrib.items():
            if name.rsplit("}", 1)[-1] == "href" and not value.startswith("#"):
                raise ValueError("External sketch reference")
    for reference in re.findall(r"url\s*\((.*?)\)", svg, re.IGNORECASE | re.DOTALL):
        if not reference.strip().strip("\"'").startswith("#"):
            raise ValueError("External sketch resource")
    return svg.encode("utf-8")


def mermaid_blocks(source: str) -> tuple[tuple[int, int, str], ...]:
    """Find only closed Mermaid fences, never interpreting an example nested in another code block."""
    blocks = []
    fence = language = ""
    start = body = offset = 0
    for line in source.splitlines(keepends=True):
        match = _FENCE.match(line.rstrip("\r\n"))
        if match and not fence:
            fence, language = match.groups()
            start, body = offset, offset + len(line)
        elif match and match[1][0] == fence[:1] and len(match[1]) >= len(fence) and not match[2].strip():
            text = source[body:offset]
            if language.strip().casefold() == "mermaid" and 0 < len(text) <= MAX_DIAGRAM_CHARACTERS:
                blocks.append((start, offset + len(line), text))
                if len(blocks) == MAX_DIAGRAMS:
                    break
            fence = language = ""
        offset += len(line)
    return tuple(blocks)


_HTML = """<!doctype html><html><head><meta charset="utf-8">
<meta http-equiv="Content-Security-Policy" content="default-src 'none'; script-src 'self' 'nonce-mluva';
style-src 'unsafe-inline'; img-src 'none'; connect-src 'none'; font-src 'none'; frame-src 'none';
object-src 'none'; base-uri 'none'; form-action 'none'">
<style>html,body { margin:0; padding:0; background:transparent; font:12px monospace; }
section { margin:8px 0; } svg { display:block; max-width:100%; height:auto; margin:auto; }
</style><script src="mermaid.min.js"></script></head><body>
<script nonce="mluva">
let latest = 0;
async function draw(revision, diagrams, dark, ink, accent) {
  latest = revision;
  mermaid.initialize({startOnLoad:false, securityLevel:'strict', suppressErrorRendering:true,
    logLevel:'fatal', maxTextSize:12000, maxEdges:100, theme:'base', look:'classic',
    fontFamily:'monospace', htmlLabels:false,
    themeVariables:{darkMode:dark, fontFamily:'monospace', primaryColor:'transparent',
      primaryTextColor:ink, primaryBorderColor:accent, lineColor:ink, textColor:ink,
      secondaryColor:'transparent', secondaryTextColor:ink, tertiaryColor:'transparent', tertiaryTextColor:ink,
      titleColor:ink, clusterBorder:ink, edgeLabelBackground:'transparent', background:'transparent',
      actorBkg:'transparent', actorBorder:ink, actorTextColor:ink, signalColor:ink, signalTextColor:ink,
      noteBkgColor:'transparent', noteTextColor:ink, noteBorderColor:ink},
    flowchart:{htmlLabels:false}, sequence:{useMaxWidth:true},
    secure:['securityLevel','startOnLoad','maxTextSize','maxEdges','suppressErrorRendering']});
  const content = document.createDocumentFragment(), images = [];
  for (let index=0; index<diagrams.length; index++) {
    if (latest !== revision) return;
    const source = diagrams[index];
    try {
      if (source.includes('%%{') || source.trimStart().startsWith('---')) throw new Error('configuration');
      const result = await mermaid.render('sketch' + revision + '-' + index, source);
      if (latest !== revision) return;
      const section = document.createElement('section');
      section.innerHTML = result.svg;
      const svg = section.firstElementChild, box = svg.viewBox.baseVal;
      const scale = Math.min(1, 1200/box.width, 1200/box.height);
      svg.setAttribute('width', Math.max(1, Math.ceil(box.width*scale)));
      svg.setAttribute('height', Math.max(1, Math.ceil(box.height*scale)));
      images.push({index, svg:svg.outerHTML});
      content.appendChild(section);
    } catch (_) { /* Keep failed and partial source in the native editor. */ }
  }
  if (latest !== revision) return;
  document.body.replaceChildren(content);
  window.webkit.messageHandlers.rendered.postMessage(JSON.stringify({revision, images}));
}
</script></body></html>"""


class MermaidPreview(Gtk.Box):
    """Attach a debounced, offline, ephemeral renderer to an existing native editor."""

    def __init__(self, editor: MarkdownTextView) -> None:
        """Keep the native document usable when the optional renderer is unavailable or fails."""
        super().__init__(orientation=Gtk.Orientation.VERTICAL, spacing=4)
        self.editor = editor
        self.web = None
        self.loaded = False
        self.in_flight = False
        self.revision = 0
        self.pending = 0
        self.watchdog = 0
        self.source = ""
        self.blocks = ()
        self.pictures = []
        self.rendered_codes = ()
        self.rendered_indices = ()
        self.rendered_dark = None
        self.rendering_dark = None
        self.notice = Gtk.Label(xalign=0, wrap=True, css_classes=["caption", "dim-label"])
        self.append(self.notice)
        editor.get_buffer().connect("changed", self._changed)
        self.connect("map", self._changed)
        self.connect("unmap", self._unmapped)
        Adw.StyleManager.get_default().connect_object("notify::dark", MermaidPreview._theme_changed, self)
        self._changed()

    def _theme_changed(self, *_args) -> None:
        """Render sketches with the current text color when the desktop changes appearance."""
        self._changed()

    def _changed(self, *_args) -> None:
        self.blocks = mermaid_blocks(self.editor.get_text()) if self.editor.markdown else ()
        self.set_visible(bool(self.blocks))
        if self.pending:
            GLib.source_remove(self.pending)
            self.pending = 0
        if (
            self.blocks
            and tuple(block[2] for block in self.blocks) == self.rendered_codes
            and self.rendered_dark == Adw.StyleManager.get_default().get_dark()
            and self.pictures
        ):
            self.editor.set_diagram_ranges(
                self.editor.get_text(), [(self.blocks[i][0], self.blocks[i][1]) for i in self.rendered_indices]
            )
            for picture in self.pictures:
                picture.set_visible(True)
            return
        for picture in self.pictures:
            picture.set_visible(False)
        if self.blocks and self.get_mapped():
            self.pending = GLib.timeout_add(450, self._render)

    def _unmapped(self, *_args) -> None:
        if self.pending:
            GLib.source_remove(self.pending)
            self.pending = 0
        self.revision += 1
        self.in_flight = False
        self._clear_watchdog()

    def _render(self) -> bool:
        self.pending = 0
        if self.in_flight or not self.get_mapped() or not self.blocks:
            return GLib.SOURCE_REMOVE
        directory = Path(__file__).parents[1] / "resources/mermaid"
        if WebKit is None or not (directory / "mermaid.min.js").is_file():
            self.notice.set_label("Mermaid preview needs WebKitGTK 6.0. The editable sketch source is kept above.")
            self.notice.set_visible(True)
            return GLib.SOURCE_REMOVE
        if self.web is None:
            manager = WebKit.UserContentManager()
            manager.register_script_message_handler("rendered", None)
            manager.connect("script-message-received::rendered", self._rendered)
            self.web = WebKit.WebView(
                network_session=WebKit.NetworkSession.new_ephemeral(), user_content_manager=manager
            )
            self.web.set_background_color(Gdk.RGBA(red=0, green=0, blue=0, alpha=0))
            # WebKit measures Mermaid; native textures display the result. This also
            # works with GTK's software renderer and keeps browser input out of documents.
            self.web.set_size_request(-1, 1)
            self.web.set_opacity(0)
            self.web.set_can_target(False)
            self.web.set_focusable(False)
            self.web.get_settings().set_enable_html5_local_storage(False)
            self.web.get_settings().set_enable_page_cache(False)
            self.web.connect("decide-policy", self._policy)
            self.web.connect("load-changed", self._loaded)
            self.web.connect("web-process-terminated", self._failed)
            self.append(self.web)
            self.web.load_html(_HTML, directory.as_uri() + "/")
        if self.loaded:
            self.in_flight = True
            self.revision += 1
            self.source = self.editor.get_text()
            self.blocks = mermaid_blocks(self.source)
            self.rendering_dark = Adw.StyleManager.get_default().get_dark()
            color = self.editor.get_color()
            ink = "#{:02x}{:02x}{:02x}".format(
                *(round(channel * 255) for channel in (color.red, color.green, color.blue))
            )
            payload = json.dumps(
                [
                    self.revision,
                    [block[2] for block in self.blocks],
                    self.rendering_dark,
                    ink,
                    ink,
                ]
            )
            self.web.evaluate_javascript("draw(..." + payload + ")", -1, None, None, None, None, None)
            self._clear_watchdog()
            self.watchdog = GLib.timeout_add_seconds(8, self._timed_out)
        return GLib.SOURCE_REMOVE

    def _loaded(self, _web, event) -> None:
        if event == WebKit.LoadEvent.FINISHED:
            self.loaded = True
            self._render()

    def _policy(self, _web, decision, kind) -> bool:
        if kind in (WebKit.PolicyDecisionType.NAVIGATION_ACTION, WebKit.PolicyDecisionType.NEW_WINDOW_ACTION):
            uri = decision.get_navigation_action().get_request().get_uri()
            base = (Path(__file__).parents[1] / "resources/mermaid").as_uri() + "/"
            if uri not in ("about:blank", base):
                decision.ignore()
                return True
        return False

    def _rendered(self, _manager, message) -> None:
        result = json.loads(message.to_string())
        if result["revision"] != self.revision:
            return
        self.in_flight = False
        self._clear_watchdog()
        if self.source != self.editor.get_text() or self.rendering_dark != Adw.StyleManager.get_default().get_dark():
            self._changed()
            return
        for picture in self.pictures:
            self.remove(picture)
        self.pictures.clear()
        rendered = []
        for item in result["images"]:
            try:
                texture = Gdk.Texture.new_from_bytes(GLib.Bytes.new(native_svg(item["svg"])))
            except (GLib.Error, ValueError, ET.ParseError):
                continue
            picture = Gtk.Picture.new_for_paintable(texture)
            picture.set_can_shrink(True)
            picture.set_vexpand(False)
            picture.set_content_fit(Gtk.ContentFit.CONTAIN)
            picture.update_property([Gtk.AccessibleProperty.LABEL], ["Mermaid sketch; editable source above"])
            self.pictures.append(picture)
            self.append(picture)
            rendered.append(item["index"])
        self.editor.set_diagram_ranges(self.source, [(self.blocks[i][0], self.blocks[i][1]) for i in rendered])
        self.rendered_codes = tuple(block[2] for block in self.blocks)
        self.rendered_indices = tuple(rendered)
        self.rendered_dark = self.rendering_dark
        self.notice.set_label("" if len(rendered) == len(self.blocks) else "Incomplete sketch — source kept above.")
        self.notice.set_visible(bool(self.notice.get_label()))

    def _clear_watchdog(self) -> None:
        if self.watchdog:
            GLib.source_remove(self.watchdog)
            self.watchdog = 0

    def _timed_out(self) -> bool:
        self.watchdog = 0
        self.web.terminate_web_process()
        return GLib.SOURCE_REMOVE

    def _failed(self, *_args) -> None:
        self._clear_watchdog()
        self.loaded = False
        self.in_flight = False
        self.editor.set_diagram_ranges(self.editor.get_text(), [])
        self.notice.set_label("Sketch preview unavailable. The editable Mermaid source is kept above.")
        self.notice.set_visible(True)
        if self.web is not None:
            self.remove(self.web)
            self.web = None
