import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "./mluva.dictation" as Mluva

ShellRoot {
    id: root
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    FloatingWindow {
        id: stage
        title: "Mluva private feature capture"
        visible: true
        implicitWidth: 1920
        implicitHeight: 1080
        color: "#242b36"
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                GradientStop { position: 0; color: "#343e4c" }
                GradientStop { position: 1; color: "#202630" }
            }
        }
        Mluva.Widget {
            id: widget
            visible: false
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
    }
    IpcHandler {
        target: "features"
        function state(): string {
            return JSON.stringify({width: stage.width, height: stage.height, phase: widget.phase,
                visible: root.overlay.visible, preview: widget.preview, message: widget.message,
                overlayWidth: root.overlay.width, overlayHeight: root.overlay.height});
        }
        function layout(): void {
            root.overlay.anchors.left = true;
            root.overlay.margins.left = 710;
            root.overlay.margins.bottom = 190;
        }
        function copy(): void { root.overlay.act("copy", ""); }
        function open(): void { root.overlay.act("open", ""); }
        function overlayImage(path: string): void {
            root.overlay.contentItem.children.find(item => item.objectName === "overlay-surface")
                .grabToImage(result => result.saveToFile(path));
        }
    }
}
