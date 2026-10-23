#!/usr/bin/env python3
"""A synthetic text probe that logs what the application itself receives.

    probe.py LOG_FILE

One JSON line per event: keys with their modifier state, clicks on the text and on a button,
the buffer text and the selection bounds after every change. The text view is a normal GTK
text field, so it is also an AT-SPI insertion target. `SIGUSR1` logs the current state.
"""

import json
import signal
import sys
import time

import gi

gi.require_version("Gtk", "4.0")
gi.require_version("Gdk", "4.0")
from gi.repository import Gdk, GLib, Gtk

LOG = open(sys.argv[1], "a", buffering=1)  # noqa: SIM115 - open for the probe's lifetime
TEXT = "alpha beta gamma delta epsilon zeta\neta theta iota kappa lambda mu\n"
CSS = b"textview, textview text { font: 22px monospace; } .target { font-size: 20px; }"


def log(kind, **fields):
    LOG.write(json.dumps({"t": time.monotonic(), "kind": kind, **fields}) + "\n")


def modifiers(state):
    names = []
    for name, flag in (
        ("shift", Gdk.ModifierType.SHIFT_MASK),
        ("ctrl", Gdk.ModifierType.CONTROL_MASK),
        ("alt", Gdk.ModifierType.ALT_MASK),
        ("super", Gdk.ModifierType.SUPER_MASK),
    ):
        if state & flag:
            names.append(name)
    return names


def build(app):
    window = Gtk.ApplicationWindow(application=app, title="Synthetic probe")
    provider = Gtk.CssProvider()
    provider.load_from_data(CSS)
    Gtk.StyleContext.add_provider_for_display(
        window.get_display(), provider, Gtk.STYLE_PROVIDER_PRIORITY_APPLICATION
    )
    box = Gtk.Box(
        orientation=Gtk.Orientation.VERTICAL,
        spacing=16,
        margin_top=24,
        margin_start=24,
        margin_end=24,
        margin_bottom=24,
    )
    view = Gtk.TextView(wrap_mode=Gtk.WrapMode.NONE, vexpand=True)
    buffer = view.get_buffer()
    buffer.set_text(TEXT)
    buffer.place_cursor(buffer.get_end_iter())
    button = Gtk.Button(label="Click target")
    button.add_css_class("target")
    button.set_size_request(260, 64)
    box.append(view)
    box.append(button)
    window.set_child(box)

    def selection():
        bounds = buffer.get_selection_bounds()
        cursor = buffer.get_iter_at_mark(buffer.get_insert()).get_offset()
        if bounds:
            start, end = bounds
            return {
                "selected": True,
                "start": start.get_offset(),
                "end": end.get_offset(),
                "text": buffer.get_text(start, end, False),
                "cursor": cursor,
            }
        return {"selected": False, "cursor": cursor}

    def state(reason):
        start, end = buffer.get_bounds()
        log(
            "state",
            reason=reason,
            text=buffer.get_text(start, end, False),
            selection=selection(),
        )

    keys = Gtk.EventControllerKey()
    keys.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)
    keys.connect(
        "key-pressed",
        lambda _c, keyval, code, st: (
            log(
                "key",
                action="press",
                name=Gdk.keyval_name(keyval),
                code=code,
                mods=modifiers(st),
            )
            or False
        ),
    )
    keys.connect(
        "key-released",
        lambda _c, keyval, code, st: log(
            "key",
            action="release",
            name=Gdk.keyval_name(keyval),
            code=code,
            mods=modifiers(st),
        ),
    )
    window.add_controller(keys)

    clicks = Gtk.GestureClick(button=0)
    clicks.set_propagation_phase(Gtk.PropagationPhase.CAPTURE)

    def pressed(gesture, count, x, y):
        log(
            "click",
            action="press",
            button=gesture.get_current_button(),
            x=round(x),
            y=round(y),
            count=count,
            mods=modifiers(gesture.get_current_event_state()),
        )

    def released(gesture, count, x, y):
        log(
            "click",
            action="release",
            button=gesture.get_current_button(),
            x=round(x),
            y=round(y),
            mods=modifiers(gesture.get_current_event_state()),
        )
        GLib.timeout_add(50, lambda: state("click") or False)

    clicks.connect("pressed", pressed)
    clicks.connect("released", released)
    view.add_controller(clicks)
    button.connect("clicked", lambda _b: log("button", action="clicked"))
    buffer.connect("changed", lambda _b: state("changed"))
    buffer.connect(
        "mark-set",
        lambda _b, _i, mark: (
            mark.get_name() == "selection_bound"
            and GLib.idle_add(lambda: state("selection") or False)
        ),
    )
    signal.signal(
        signal.SIGUSR1, lambda *_: GLib.idle_add(lambda: state("signal") or False)
    )
    window.present()
    view.grab_focus()
    log("ready")


app = Gtk.Application(application_id="synthetic.probe")
app.connect("activate", build)
app.run([sys.argv[0]])
