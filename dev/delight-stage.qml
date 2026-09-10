import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "./mluva.dictation" as Mluva

ShellRoot {
    id: root
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    property string wallpaper: "wallpaper-nord.jpg"
    property string themeName: "NORD"
    property bool stageSaved: false
    FloatingWindow {
        id: stage
        title: "Mluva private Omarchy desktop"
        visible: true
        implicitWidth: 1920
        implicitHeight: 1080
        color: Color.background
        Image { anchors.fill: parent; source: root.wallpaper; fillMode: Image.PreserveAspectCrop }
        Rectangle { anchors.fill: parent; color: Color.background; opacity: 0.18 }
        Rectangle {
            width: parent.width; height: 30; color: Color.background
            Row {
                x: 18; anchors.verticalCenter: parent.verticalCenter; spacing: 18
                Text { text: "◆"; color: Color.accent; font.pixelSize: 16 }
                Repeater {
                    model: ["1", "2", "3", "4"]
                    Text { required property string modelData; text: modelData; color: modelData === "1" ? Color.accent : Color.muted; font.family: "Adwaita Sans"; font.pixelSize: 13 }
                }
            }
            Text { anchors.centerIn: parent; text: "OMARCHY"; color: Color.foreground; font.family: "Adwaita Sans"; font.pixelSize: 12; font.letterSpacing: 1.5 }
            Row {
                anchors.right: parent.right; anchors.rightMargin: 20; anchors.verticalCenter: parent.verticalCenter; spacing: 22
                Text { text: root.themeName; color: Color.accent; font.family: "Adwaita Sans"; font.pixelSize: 12 }
                Text { text: "09:41"; color: Color.foreground; font.family: "Adwaita Sans"; font.pixelSize: 13 }
            }
        }
        Mluva.Widget {
            id: widget
            x: 1750; y: 4
            visible: false
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
    }
    IpcHandler {
        target: "delight"
        function state(): string {
            return JSON.stringify({width: stage.width, height: stage.height, phase: widget.phase,
                preview: widget.preview, overlayVisible: root.overlay.visible,
                foreground: Color.foreground.toString(), background: Color.background.toString(),
                overlayWidth: root.overlay.width, overlayHeight: root.overlay.height, theme: root.themeName,
                stageSaved: root.stageSaved});
        }
        function layout(): void {
            root.overlay.anchors.left = true;
            root.overlay.margins.left = 710;
            root.overlay.margins.bottom = 90;
        }
        function copy(): void { root.overlay.act("copy", ""); }
        function open(): void { root.overlay.act("open", ""); }
        function overlayImage(path: string): void {
            root.overlay.contentItem.children.find(item => item.objectName === "overlay-surface")
                .grabToImage(result => result.saveToFile(path));
        }
        function stageImage(path: string): void {
            root.stageSaved = false;
            stage.contentItem.grabToImage(result => { root.stageSaved = result.saveToFile(path); });
        }
        function theme(name: string, colors: string): void {
            root.themeName = name.toUpperCase().replace(/-/g, " ");
            root.wallpaper = "wallpaper-" + name + ".jpg";
            Color.loadColors(colors);
            Color.loadShell("");
        }
    }
}
