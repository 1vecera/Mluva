import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "./mluva.dictation" as Mluva

ShellRoot {
    id: root
    readonly property bool portrait: Quickshell.env("CAMPAIGN_PORTRAIT") === "1"
    readonly property bool task: Quickshell.env("CAMPAIGN_SCENARIO").includes("task")
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    property bool stageSaved: false
    FloatingWindow {
        id: stage
        title: "Mluva campaign desktop"
        visible: true
        implicitWidth: root.portrait ? 1080 : 1920
        implicitHeight: root.portrait ? 1920 : 1080
        color: Color.background
        Item {
        id: editorial
        anchors.fill: parent
        Image {
            anchors.fill: parent
            source: "wallpaper.jpg"
            fillMode: Image.PreserveAspectCrop
        }
        Rectangle { anchors.fill: parent; color: "#191c23"; opacity: 0.50 }
        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: "#dd191c23" }
                GradientStop { position: 0.55; color: "#18191c23" }
                GradientStop { position: 1; color: "#08191c23" }
            }
        }
        Row {
            x: 64; y: 58; spacing: 17
            Image { width: 42; height: 42; source: "mark.svg" }
            Text { text: "mluva"; color: Color.foreground; font.family: "Inter"; font.pixelSize: 36; font.weight: Font.DemiBold }
        }
        Text {
            x: 64; y: root.portrait ? 170 : 234
            text: "YOUR VOICE, IN FOCUS"
            color: Color.accent; font.family: "Inter"; font.pixelSize: 17; font.letterSpacing: 2.8
        }
        Text {
            x: 60; y: root.portrait ? 222 : 280
            width: root.portrait ? 940 : 490
            text: root.task ? "Think aloud.\nMake it clear." : "Great words.\nA fresh page."
            color: Color.foreground; font.family: "Inter"; font.pixelSize: root.portrait ? 82 : 68
            font.weight: Font.DemiBold; font.letterSpacing: -2.5; lineHeight: 0.98
        }
        Text {
            x: 64; y: root.portrait ? 426 : 472
            width: root.portrait ? 940 : 440
            text: root.task ? "A spoken idea becomes a task spec.\nThe missing details stay visible." : "An archival voice. Real transcription.\nYour original, kept alongside every edit."
            color: "#adb5c4"; font.family: "Inter"; font.pixelSize: 23; lineHeight: 1.25
        }
        Rectangle { x: 64; y: root.portrait ? 1750 : 942; width: root.portrait ? 952 : 1792; height: 1; color: Color.foreground; opacity: 0.2 }
        Text {
            x: 64; y: root.portrait ? 1778 : 970
            text: Quickshell.env("CAMPAIGN_BUILD_LABEL") + "  /  OMARCHY · NORD"
            color: Color.foreground; font.family: "Inter"; font.pixelSize: 18; font.letterSpacing: 1.3
        }
        Text {
            x: 64; y: root.portrait ? 1820 : 1007
            text: root.task ? "Fish synthetic speech → " + (Quickshell.env("CAMPAIGN_STT") === "voxtype" ? "Voxtype" : "Scribe") + (Quickshell.env("CAMPAIGN_SCENARIO").startsWith("saved") ? " → saved task draft" : " → live task spec") : "JFK · Rice University, 1962 · public-domain recording"
            color: "#adb5c4"; font.family: "Inter"; font.pixelSize: 16
        }
        Text {
            x: 64; y: root.portrait ? 1857 : 1035
            text: (Quickshell.env("CAMPAIGN_BUILD_LABEL") === "v0.3.0" ? "Released production UI" : "Experimental production UI") + " · automated controls · isolated desktop"
            color: "#adb5c4"; font.family: "Inter"; font.pixelSize: 14
        }
        Mluva.Widget {
            id: widget
            x: root.portrait ? 914 : 1722; y: 66
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
        }
    }
    IpcHandler {
        target: "campaign"
        function layout(): void {
            const window = root.overlay.contentItem.Window.window;
            window.x = root.portrait ? 290 : 64;
            window.y = Qt.binding(() => root.overlay.screen.height - root.overlay.height - (root.portrait ? 224 : 160));
        }
        function state(): string {
            return JSON.stringify({width: stage.width, height: stage.height, phase: widget.phase,
                preview: widget.preview, overlayVisible: root.overlay.visible,
                foreground: Color.foreground.toString(), background: Color.background.toString(),
                overlayWidth: root.overlay.width, overlayHeight: root.overlay.height, stageSaved: root.stageSaved});
        }
        function copy(): void { root.overlay.act("copy", ""); }
        function dismiss(): void { root.overlay.act("dismiss", ""); }
        function overlayImage(path: string): void {
            root.overlay.contentItem.children.find(item => item.objectName === "overlay-surface")
                .grabToImage(result => result.saveToFile(path));
        }
        function stageImage(path: string): void {
            root.stageSaved = false;
            widget.visible = false;
            editorial.grabToImage(result => { root.stageSaved = result.saveToFile(path); widget.visible = true; });
        }
    }
}
