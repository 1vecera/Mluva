"""The session-bus front of the accessibility bus, as at-spi-bus-launcher provides on a desktop.

The private desktop runs its own AT-SPI bus and registry but nothing on the session bus says so.
This owns `org.a11y.Bus`, reports `IsEnabled` and returns the private bus's address, as the
repository's Firefox text peer does, so Firefox exposes its fields and Mluva tracks the focused one.
"""

import os

import gi

gi.require_version("Gio", "2.0")
from gi.repository import Gio, GLib

XML = """<node>
<interface name='org.a11y.Status'>
  <property name='IsEnabled' type='b' access='read'/>
  <property name='ScreenReaderEnabled' type='b' access='read'/>
</interface>
<interface name='org.a11y.Bus'>
  <method name='GetAddress'><arg type='s' direction='out'/></method>
</interface>
</node>"""


def main() -> None:
    info = Gio.DBusNodeInfo.new_for_xml(XML)
    address = os.environ["AT_SPI_BUS_ADDRESS"]

    def get_property(_c, _s, _p, _i, name):
        return GLib.Variant("b", name == "IsEnabled")

    def call(_c, _s, _p, _i, _method, _params, invocation):
        invocation.return_value(GLib.Variant("(s)", (address,)))

    def acquired(connection, _name):
        connection.register_object(
            "/org/a11y/bus", info.interfaces[0], None, get_property, None
        )
        connection.register_object(
            "/org/a11y/bus", info.interfaces[1], call, None, None
        )

    Gio.bus_own_name(
        Gio.BusType.SESSION,
        "org.a11y.Bus",
        Gio.BusNameOwnerFlags.NONE,
        acquired,
        lambda *_: print("a11y-status-ready", flush=True),
        lambda *_: print("a11y-status-lost", flush=True),
    )
    GLib.MainLoop().run()


main()
