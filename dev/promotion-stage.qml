import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "./mluva.dictation" as Mluva

ShellRoot {
    id: root
    property string chapter: "intro"
    readonly property bool portrait: Quickshell.env("MLUVA_PROMO_FORMAT") === "portrait"
    readonly property bool light: Quickshell.env("MLUVA_PROMO_THEME") === "rose-pine"
    readonly property color ink: light ? "#19342f" : "#edf5ef"
    readonly property color muted: light ? "#536b62" : "#a9bfb6"
    readonly property color accent: light ? "#287e6b" : "#91e6c8"
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    readonly property var chapters: ({
        intro: ["Speak freely.\nStay in flow.", "Mluva brings your voice\nto your Omarchy desktop.", "DICTATION FOR OMARCHY"],
        recording: ["Think out loud.", "Five lines of live words.\nA little room to think.", "SPEAK"],
        review: ["Your thought,\nready to use.", "Polish it. Structure it.\nKeep your own voice.", "SHAPE"],
        rewriting: ["From ramble\nto a clear note.", "Rewrite from the widget.\nKeep the original.", "SHAPE"],
        saved: ["Keep the thought.\nAnd every version.", "Search your history.\nCopy the version you want.", "KEEP"],
        end: ["Make room\nfor your voice.", "Open source.\nMade to feel at home.", "TRY MLUVA"]
    })
    FloatingWindow {
        id: stage
        title: "Mluva promotion stage"
        visible: true
        implicitWidth: root.portrait ? 1080 : 1920
        implicitHeight: root.portrait ? 1920 : 1080
        color: root.light ? "#f1f6f3" : "#0f1916"

        Rectangle {
            anchors.fill: parent
            gradient: Gradient {
                orientation: Gradient.Horizontal
                GradientStop { position: 0; color: root.light ? "#f1f6f3" : "#0f1916" }
                GradientStop { position: 1; color: root.light ? "#e2eee7" : "#20352d" }
            }
        }
        Rectangle {
            x: root.portrait ? 68 : 700
            y: root.portrait ? 608 : 148
            width: root.portrait ? 944 : 1124
            height: root.portrait ? 838 : 804
            color: "transparent"
            border.width: 1
            border.color: root.light ? "#c2d6c8" : "#344d40"
        }
        Row {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 72 : 62
            spacing: 20
            Image {
                width: 44
                height: 44
                source: "mark.svg"
            }
            Text {
                text: "mluva"
                color: root.ink
                font.family: "Inter"
                font.pixelSize: 38
                font.weight: Font.DemiBold
                font.letterSpacing: -1
            }
        }
        Text {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 192 : 250
            text: root.chapters[root.chapter][2]
            color: root.accent
            font.family: "Inter"
            font.pixelSize: root.portrait ? 22 : 18
            font.weight: Font.DemiBold
            font.letterSpacing: 3
        }
        Text {
            x: root.portrait ? 68 : 92
            y: root.portrait ? 252 : 300
            width: root.portrait ? 950 : 590
            text: root.chapters[root.chapter][0]
            color: root.ink
            font.family: "Inter"
            font.pixelSize: root.portrait ? 84 : 68
            font.weight: Font.DemiBold
            font.letterSpacing: -3
            wrapMode: Text.WordWrap
            lineHeight: 0.98
        }
        Text {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 470 : 530
            text: root.chapters[root.chapter][1]
            color: root.muted
            font.family: "Inter"
            font.pixelSize: root.portrait ? 29 : 26
            lineHeight: 1.2
        }
        Rectangle {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 1744 : 685
            width: root.portrait ? 936 : 520
            height: 1
            color: root.light ? "#c2d6c8" : "#344d40"
        }
        Text {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 1764 : 715
            text: root.chapter === "end" ? "github.com/1vecera/Mluva" : "Speak. Shape. Keep."
            color: root.accent
            font.family: "Inter"
            font.pixelSize: root.portrait ? 31 : 24
            font.weight: Font.Medium
        }
        Text {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 1835 : 972
            text: "OPEN SOURCE  /  v0.1.1"
            color: root.muted
            font.family: "Inter"
            font.pixelSize: root.portrait ? 19 : 16
            font.letterSpacing: 2
        }
        Text {
            x: root.portrait ? 72 : 96
            y: root.portrait ? 1880 : 1010
            text: "Scripted demo · Omarchy integration preview"
            color: root.muted
            font.family: "Inter"
            font.pixelSize: root.portrait ? 19 : 15
        }
        Mluva.Widget {
            id: widget
            anchors.top: parent.top
            anchors.right: parent.right
            anchors.topMargin: root.portrait ? 84 : 76
            anchors.rightMargin: root.portrait ? 72 : 96
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
    }
    IpcHandler {
        target: "promotion"
        function chapter(value: string): void { root.chapter = value; }
        function theme(colors: string): void {
            Color.loadColors(colors);
            root.overlay.margins.bottom = root.portrait ? 200 : 24;
        }
        function state(): string {
            return JSON.stringify({chapter: root.chapter, phase: widget.phase,
                width: stage.width, height: stage.height, overlayVisible: root.overlay.visible,
                previewLines: root.overlay.previewLines, previewLength: widget.preview.length});
        }
        function structure(): void { root.overlay.act("rewrite", "structure"); }
        function copy(): void { root.overlay.act("copy", ""); }
        function dismiss(): void { root.overlay.act("dismiss", ""); }
        function overlayImage(path: string): void {
            const surface = root.overlay.contentItem.children.find(item => item.objectName === "overlay-surface");
            surface.grabToImage(result => result.saveToFile(path));
        }
    }
}
