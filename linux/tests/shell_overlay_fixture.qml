import QtQuick
import Quickshell
import Quickshell.Io
import "../quickshell/mluva.dictation" as Mluva

ShellRoot {
    id: root
    FloatingWindow {
        id: editor
        title: "Mluva overlay fixture"
        screen: Quickshell.screens[0]
        visible: true
        implicitWidth: 300
        implicitHeight: 100
        TextInput {
            id: input
            text: "Synthetic editor keeps keyboard focus"
            anchors.centerIn: parent
            focus: true
        }
        Mluva.Widget {
            id: widget
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
    }
    function snapshot(): string {
            const overlay = widget.data.find(item => item.objectName === "mluva-recording-overlay");
            return JSON.stringify({phase: widget.phase, preview: widget.preview, elapsed: widget.elapsed,
                visible: overlay.visible, focusable: overlay.focusable, mask: overlay.mask !== null,
                width: overlay.width, height: overlay.height, screenWidth: overlay.screen.width,
                screenHeight: overlay.screen.height, bottom: overlay.margins.bottom, focus: input.activeFocus});
    }
    Timer {
        interval: 80
        running: true
        repeat: true
        onTriggered: console.warn("MLUVA_SNAPSHOT " + root.snapshot())
    }
}
