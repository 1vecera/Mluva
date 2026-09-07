import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "../quickshell/mluva.dictation" as Mluva

ShellRoot {
    id: root
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    function descendants(item): var {
        return [item].concat(...(item.children || []).map(child => descendants(child)));
    }
    IpcHandler {
        target: "fixture"
        function click(name: string): void {
            if (name === "more") root.overlay.menuOpen = true;
            else root.descendants(root.overlay.contentItem).find(item => item.objectName === name + "-button").clicked();
        }
        function theme(light: bool): void {
            Color.loadColors(light ? 'background = "#faf4ed"\nforeground = "#575279"\naccent = "#286983"\nred = "#b4637a"'
                : 'background = "#1a1b26"\nforeground = "#c0caf5"\naccent = "#7aa2f7"\nred = "#f7768e"');
        }
        function option(index: int): void {
            const menu = root.overlay.data.find(item => item.objectName === "rewrite-menu");
            root.descendants(menu.contentItem).find(item => item.objectName === "rewrite-option-" + index).clicked();
        }
    }
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
            const overlay = root.overlay;
            const text = root.descendants(overlay.contentItem).find(item => item.objectName === "transcript-text");
            const viewport = root.descendants(overlay.contentItem).find(item => item.objectName === "transcript-viewport");
            const copy = root.descendants(overlay.contentItem).find(item => item.objectName === "copy-button");
            return JSON.stringify({phase: widget.phase, preview: widget.preview, elapsed: widget.elapsed,
                identifier: widget.identifier, options: widget.options, menuOpen: overlay.menuOpen,
                copyEnabled: copy.enabled, renderedText: text.text,
                textHeight: text.height, textY: text.y, viewportHeight: viewport.height, lineHeight: overlay.lineHeight,
                background: Color.popups.background.toString(), ink: Color.popups.text.toString(),
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
