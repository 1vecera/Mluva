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
    readonly property int reviewDuration: 8000
    property real remaining: reviewDuration
    property double lastTick: Date.now()
    property bool dismissed: false
    readonly property bool countdownPaused: reviewHover.hovered || menuOpen || surface.Window.active
    readonly property bool reviewing: ["ready", "rewriting", "review-error"].includes(phase)
    readonly property bool busy: phase === "rewriting"
    readonly property bool active: reviewing || ["preparing", "recording", "processing", "error"].includes(phase)
    readonly property color emphasis: phase === "recording" || phase === "error" || phase === "review-error"
        ? Color.urgent : Color.accent
    readonly property int textSize: Math.max(12, Style.font.body)
    readonly property int lineHeight: Math.ceil(textSize * 1.4)
    readonly property string status: ({"preparing": "Preparing microphone…", "recording": "Recording",
        "processing": "Transcribing…", "error": "Dictation failed · open Mluva"})[phase] || ""
    readonly property string timer: Math.floor(elapsed / 60).toString().padStart(2, "0")
        + ":" + (elapsed % 60).toString().padStart(2, "0")
    signal review(string action, string style)

    function act(action, style) {
        menuOpen = false;
        if (action === "dismiss" || action === "open") dismissed = true;
        remaining = reviewDuration;
        review(action, style || "");
    }
    function resetCountdown() {
        remaining = reviewDuration;
        lastTick = Date.now();
        dismissed = false;
    }
    onPhaseChanged: {
        if (!reviewing || busy) menuOpen = false;
        resetCountdown();
    }
    onIdentifierChanged: { menuOpen = false; resetCountdown(); }
    onMessageChanged: resetCountdown()
    onCountdownPausedChanged: lastTick = Date.now()
    visible: active && !dismissed
    anchors.bottom: true
    margins.bottom: 24 + (bar && bar.position === "bottom" ? bar.barSize : 0)
    implicitWidth: Math.min(500, screen ? screen.width - 32 : 500)
    implicitHeight: body.implicitHeight + 20
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

    Timer {
        interval: 40
        repeat: true
        running: root.visible && root.reviewing && !root.busy && !root.countdownPaused
        onTriggered: {
            const now = Date.now();
            root.remaining = Math.max(0, root.remaining - Math.max(0, now - root.lastTick));
            root.lastTick = now;
            if (root.remaining === 0) root.act("dismiss");
        }
        onRunningChanged: root.lastTick = Date.now()
    }

    BorderSurface {
        id: surface
        anchors.fill: parent
        color: Qt.alpha(Color.popups.background, 0.93)
        radius: Style.cornerRadius
        borderSpec: Border.surfaceSpec("popups", "border", Color.popups.border, 1)
        Keys.onEscapePressed: root.menuOpen ? root.menuOpen = false : root.act("dismiss")
        HoverHandler { id: reviewHover }
        Column {
            id: body
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: parent.top
            anchors.margins: 10
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
                    font.pixelSize: root.textSize
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
                    id: dismiss
                    objectName: "dismiss-button"
                    text: ""
                    implicitWidth: 30
                    implicitHeight: 30
                    tooltipText: root.busy ? "Dismiss" : "Dismiss · closes after 8 idle seconds"
                    Accessible.name: "Dismiss review"
                    onClicked: root.act("dismiss")
                    Canvas {
                        id: countdownRing
                        anchors.centerIn: parent
                        width: 22
                        height: 22
                        property real fraction: root.remaining / root.reviewDuration
                        property color ink: Color.popups.text
                        onFractionChanged: requestPaint()
                        onInkChanged: requestPaint()
                        onPaint: {
                            const ctx = getContext("2d");
                            ctx.reset();
                            ctx.strokeStyle = ink;
                            ctx.lineWidth = 1.3;
                            ctx.globalAlpha = 0.7;
                            ctx.beginPath(); ctx.moveTo(8, 8); ctx.lineTo(14, 14);
                            ctx.moveTo(14, 8); ctx.lineTo(8, 14); ctx.stroke();
                            if (!root.busy) {
                                ctx.globalAlpha = 0.4;
                                ctx.beginPath(); ctx.arc(11, 11, 9, -Math.PI / 2, -Math.PI / 2 + 2 * Math.PI * fraction);
                                ctx.stroke();
                            }
                        }
                        Connections { target: root; function onBusyChanged() { countdownRing.requestPaint(); } }
                    }
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
