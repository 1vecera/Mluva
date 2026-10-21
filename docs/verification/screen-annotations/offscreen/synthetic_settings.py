#!/usr/bin/env python3
"""A synthetic settings page with two deliberate layout faults to annotate.

Every widget sits at a fixed position, so the driver knows where to draw. When the file named
by the first argument appears, an error banner covers the title: a scripted screen change.
"""
import os
import sys

import gi

gi.require_version("Gtk", "4.0")
from gi.repository import GLib, Gtk  # noqa: E402

CSS = b"""
window { background: #f4f1ea; }
.card { background: #ffffff; border-radius: 14px; border: 1px solid #ddd5c4; }
.title { font-size: 24px; font-weight: 700; color: #2b2a27; }
.row { font-size: 16px; color: #3d3b36; }
.save { background: #2f6f4f; color: white; font-weight: 700; border-radius: 8px; }
.banner { background: #b3261e; color: white; font-size: 18px; font-weight: 700; padding: 18px 24px; border-radius: 10px; }
"""
TRIGGER = sys.argv[1] if len(sys.argv) > 1 else None


def build(app):
    window = Gtk.ApplicationWindow(application=app, title="Synthetic settings")
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_display(
        window.get_display(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
    )
    fixed = Gtk.Fixed()
    card = Gtk.Box()
    card.add_css_class("card")
    card.set_size_request(560, 520)
    fixed.put(card, 40, 100)
    title = Gtk.Label(label="Workspace preferences")
    title.add_css_class("title")
    fixed.put(title, 70, 130)
    for index, text in enumerate(["Theme: Paper", "Font size: 16 px", "Autosave: every 30 s"]):
        label = Gtk.Label(label=text)
        label.add_css_class("row")
        fixed.put(label, 70, 200 + 44 * index)
    # Fault 1: this label runs past the card's right edge.
    long = Gtk.Label(label="Sync folder: /synthetic/projects/quarterly-planning-archive/2026")
    long.add_css_class("row")
    fixed.put(long, 70, 360)
    # Fault 2: the Save button sits outside the card.
    save = Gtk.Button(label="Save")
    save.add_css_class("save")
    save.set_size_request(120, 44)
    fixed.put(save, 470, 640)
    banner = Gtk.Label(label="Sync failed: folder unavailable")
    banner.add_css_class("banner")
    banner.set_size_request(500, 64)
    banner.set_visible(False)
    fixed.put(banner, 70, 116)

    def poll():
        if TRIGGER and os.path.exists(TRIGGER):
            banner.set_visible(True)
            return False
        return True

    GLib.timeout_add(100, poll)
    window.set_child(fixed)
    window.present()


app = Gtk.Application(application_id="synthetic.settings")
app.connect("activate", build)
app.run([sys.argv[0]])
