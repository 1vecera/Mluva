import QtQuick
import Quickshell
import Quickshell.Wayland

PanelWindow {
    id: root
    objectName: "mluva-recording-overlay"
    required property string phase
    required property int elapsed
    required property real level
    required property string preview
    property var bar
    readonly property bool active: ["preparing", "recording", "processing", "copied", "error"].includes(phase)
    readonly property color surface: bar ? bar.background : palette.window
    readonly property color ink: bar ? bar.foreground : palette.windowText
    readonly property color emphasis: phase === "recording" || phase === "error"
        ? (bar ? bar.urgent : palette.highlight) : ink
    readonly property string status: ({"preparing": "Preparing microphone…", "recording": "Recording",
        "processing": "Transcribing…", "copied": "Copied · ready to paste", "error": "Dictation failed · open Mluva"})[phase] || ""
    readonly property string timer: Math.floor(elapsed / 60).toString().padStart(2, "0")
        + ":" + (elapsed % 60).toString().padStart(2, "0")

    visible: active
    anchors.bottom: true
    margins.bottom: 24 + (bar && bar.position === "bottom" ? bar.barSize : 0)
    implicitWidth: Math.min(520, screen ? screen.width - 32 : 520)
    implicitHeight: preview.length > 0 ? 100 : 64
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    focusable: false
    mask: Region {}
    Component.onCompleted: {
        if (root.WlrLayershell != null) {
            root.WlrLayershell.namespace = "mluva-recording-overlay";
            root.WlrLayershell.layer = WlrLayer.Overlay;
            root.WlrLayershell.keyboardFocus = WlrKeyboardFocus.None;
        }
    }

    SystemPalette { id: palette }
    Rectangle {
        anchors.fill: parent
        color: root.surface
        radius: 14
        border.color: root.emphasis
        border.width: 1
        Column {
            anchors.fill: parent
            anchors.margins: 18
            spacing: 12
            Row {
                width: parent.width
                spacing: 12
                Rectangle {
                    width: 10
                    height: 10
                    radius: 5
                    anchors.verticalCenter: parent.verticalCenter
                    color: root.emphasis
                }
                Text {
                    width: parent.width - 22 - (clock.visible ? clock.width + 12 : 0)
                    text: root.status
                    color: root.emphasis
                    font.family: root.bar ? root.bar.fontFamily : "sans-serif"
                    font.pixelSize: 16
                    font.bold: true
                    elide: Text.ElideRight
                    textFormat: Text.PlainText
                }
                Text {
                    id: clock
                    visible: root.phase === "recording"
                    text: root.timer
                    color: root.ink
                    font.family: root.bar ? root.bar.fontFamily : "monospace"
                    font.pixelSize: 16
                }
            }
            Text {
                width: parent.width
                visible: root.preview.length > 0
                text: root.preview
                color: root.ink
                font.family: root.bar ? root.bar.fontFamily : "sans-serif"
                font.pixelSize: 14
                elide: Text.ElideLeft
                maximumLineCount: 1
                textFormat: Text.PlainText
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.margins: 10
            height: 3
            radius: 2
            width: (parent.width - 20) * root.level
            color: root.emphasis
            visible: root.phase === "recording"
        }
    }
}
