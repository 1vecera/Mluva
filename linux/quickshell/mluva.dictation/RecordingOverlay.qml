import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Wayland
import Quickshell.Hyprland
import qs.Commons
import qs.Ui

PanelWindow {
    id: root
    objectName: "mluva-recording-overlay"
    required property string phase
    required property int elapsed
    required property real level
    required property string preview
    property string identifier: ""
    property var options: []
    property string message: ""
    property var bar
    property bool menuOpen: false
    readonly property bool reviewing: ["ready", "rewriting", "review-error"].includes(phase)
    readonly property bool busy: phase === "rewriting"
    readonly property bool active: reviewing || ["preparing", "recording", "processing", "error"].includes(phase)
    readonly property color emphasis: phase === "recording" || phase === "error" || phase === "review-error"
        ? Color.urgent : Color.accent
    readonly property int lineHeight: Math.ceil(Style.font.title * 1.4)
    readonly property string status: ({"preparing": "Preparing microphone…", "recording": "Recording",
        "processing": "Transcribing…", "error": "Dictation failed · open Mluva"})[phase] || ""
    readonly property string timer: Math.floor(elapsed / 60).toString().padStart(2, "0")
        + ":" + (elapsed % 60).toString().padStart(2, "0")
    signal review(string action, string style)

    function act(action, style) {
        menuOpen = false;
        review(action, style || "");
    }
    onPhaseChanged: if (!reviewing || busy) menuOpen = false
    onIdentifierChanged: menuOpen = false
    visible: active
    anchors.bottom: true
    margins.bottom: 24 + (bar && bar.position === "bottom" ? bar.barSize : 0)
    implicitWidth: Math.min(520, screen ? screen.width - 32 : 520)
    implicitHeight: body.implicitHeight + 24
    color: "transparent"
    exclusionMode: ExclusionMode.Ignore
    focusable: reviewing
    mask: Region { item: root.reviewing ? surface : null }
    Component.onCompleted: {
        if (root.WlrLayershell != null) {
            root.WlrLayershell.namespace = "mluva-recording-overlay";
            root.WlrLayershell.layer = WlrLayer.Overlay;
            root.WlrLayershell.keyboardFocus = Qt.binding(() => root.reviewing ? WlrKeyboardFocus.OnDemand : WlrKeyboardFocus.None);
        }
    }

    component ActionButton: Button {
        opacity: enabled ? 1 : 0.4
        foreground: Color.popups.text
        fontSize: Style.font.body
        horizontalPadding: 8
        verticalPadding: 5
        focusable: true
    }

    BorderSurface {
        id: surface
        anchors.fill: parent
        color: Color.popups.background
        radius: Style.cornerRadius
        borderSpec: Border.surfaceSpec("popups", "border", Color.popups.border, 1)
        Keys.onEscapePressed: root.menuOpen ? root.menuOpen = false : root.act("dismiss")
        Column {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 12
            spacing: 6
            RowLayout {
                width: parent.width
                visible: !root.reviewing
                spacing: 8
                Rectangle {
                    width: 6
                    height: 6
                    radius: 3
                    color: root.emphasis
                }
                Text {
                    Layout.fillWidth: true
                    text: root.status
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Text {
                    visible: root.phase === "recording"
                    text: root.timer
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                }
            }
            Item {
                id: transcriptViewport
                objectName: "transcript-viewport"
                width: parent.width
                height: root.lineHeight * 3
                visible: root.preview.length > 0 || root.phase === "recording"
                clip: true
                // Keep three complete wrapped lines in a fixed viewport. New lines
                // move by one line; partial updates never animate or shrink the type.
                Text {
                    id: transcript
                    objectName: "transcript-text"
                    width: parent.width
                    y: Math.min(0, parent.height - height)
                    text: root.preview
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: Style.font.title
                    wrapMode: Text.Wrap
                    lineHeightMode: Text.FixedHeight
                    lineHeight: root.lineHeight
                    textFormat: Text.PlainText
                }
            }
            Text {
                width: parent.width
                visible: root.reviewing && root.message.length > 0
                text: root.message
                color: root.phase === "review-error" ? Color.urgent : Color.popups.text
                font.family: Style.font.family
                font.pixelSize: Style.font.body
                textFormat: Text.PlainText
                wrapMode: Text.Wrap
            }
            RowLayout {
                width: parent.width
                visible: root.reviewing
                spacing: 2
                ActionButton {
                    objectName: "polish-button"
                    text: "Polish"
                    visible: !root.busy
                    onClicked: root.act("rewrite", "polish")
                }
                ActionButton {
                    objectName: "structure-button"
                    text: "Structure"
                    visible: !root.busy
                    onClicked: root.act("rewrite", "structure")
                }
                ActionButton {
                    id: more
                    objectName: "more-button"
                    text: "More ▴"
                    visible: !root.busy
                    enabled: root.options.length > 0
                    selected: root.menuOpen
                    onClicked: root.menuOpen = !root.menuOpen
                }
                Text {
                    visible: root.busy
                    text: "Rewriting…"
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                }
                ActionButton {
                    text: "Cancel"
                    visible: root.busy
                    onClicked: root.act("cancel")
                }
                Item { Layout.fillWidth: true }
                ActionButton {
                    objectName: "copy-button"
                    text: "Copy"
                    enabled: !root.busy
                    onClicked: root.act("copy")
                }
                ActionButton {
                    objectName: "open-button"
                    text: "Open"
                    onClicked: root.act("open")
                }
                ActionButton {
                    objectName: "dismiss-button"
                    text: "×"
                    tooltipText: "Dismiss"
                    onClicked: root.act("dismiss")
                }
            }
        }
        Rectangle {
            anchors.left: parent.left
            anchors.bottom: parent.bottom
            anchors.margins: 1
            height: 2
            width: (parent.width - 2) * root.level
            color: Color.accent
            visible: root.phase === "recording"
        }
    }
    PopupWindow {
        id: menu
        objectName: "rewrite-menu"
        visible: root.menuOpen && root.reviewing && !root.busy
        anchor.item: more
        anchor.edges: Edges.Top | Edges.Left
        anchor.gravity: Edges.Top | Edges.Right
        implicitWidth: Math.min(260, root.width - 24)
        implicitHeight: Math.min(optionsList.contentHeight + 12, root.screen ? root.screen.height / 2 : 240)
        color: "transparent"
        onVisibleChanged: if (visible) optionsList.forceActiveFocus()
        BorderSurface {
            anchors.fill: parent
            color: Color.popups.background
            radius: Style.cornerRadius
            borderSpec: Border.surfaceSpec("popups", "border", Color.popups.border, 1)
            ListView {
                id: optionsList
                anchors.fill: parent
                anchors.margins: 6
                clip: true
                model: root.options
                currentIndex: 0
                boundsBehavior: Flickable.StopAtBounds
                keyNavigationEnabled: true
                Keys.onEscapePressed: root.menuOpen = false
                Keys.onReturnPressed: if (count) root.act("rewrite", root.options[currentIndex].value)
                delegate: ActionButton {
                    required property var modelData
                    required property int index
                    objectName: "rewrite-option-" + index
                    width: optionsList.width
                    text: ""
                    implicitHeight: Style.spacing.popupRowHeight
                    hasCursor: optionsList.currentIndex === index
                    onHovered: function(hot) { if (hot) optionsList.currentIndex = index; }
                    onClicked: root.act("rewrite", modelData.value)
                    Text {
                        anchors.fill: parent
                        anchors.leftMargin: 8
                        anchors.rightMargin: 8
                        text: modelData.label
                        color: Color.popups.text
                        font.family: Style.font.family
                        font.pixelSize: Style.font.body
                        verticalAlignment: Text.AlignVCenter
                        elide: Text.ElideRight
                        textFormat: Text.PlainText
                    }
                }
            }
        }
    }
    HyprlandFocusGrab {
        active: menu.visible && Quickshell.env("WAYLAND_DISPLAY") !== ""
        windows: [root, menu]
        onCleared: root.menuOpen = false
    }
}
