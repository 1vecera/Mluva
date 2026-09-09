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
    property int previewStart: 0
    property string identifier: ""
    property var options: []
    property string message: ""
    property var bar
    property bool menuOpen: false
    property int reviewDuration: 4000
    property bool showCopy: true
    property bool smoothScrolling: true
    property int scrollDuration: 800
    property int scrollLookahead: 2
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
    readonly property int previewLines: 5
    readonly property real surfaceOpacity: 0.82
    property bool animatePreview: false
    property bool previewReady: false
    property string displayedPreview: ""
    property int displayedStart: 0
    property string displayedIdentifier: ""
    property real leadingIndent: 0
    property real discardedHeight: 0
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
    function resetPreviewMotion() {
        animatePreview = false;
        Qt.callLater(() => { animatePreview = true; });
    }
    function syncPreview() {
        if (!previewReady) return;
        // QML strings use UTF-16; the bridge's offset counts Unicode characters.
        const previous = displayedPreview.match(/[\uD800-\uDBFF][\uDC00-\uDFFF]|[\s\S]/g) || [];
        const removed = previewStart - displayedStart;
        const overlap = previous.slice(removed, removed + 64).join("");
        const continuous = displayedPreview.length > 0 && displayedIdentifier === identifier
            && removed >= 0 && removed < previous.length && preview.startsWith(overlap);
        if (!continuous) {
            resetPreviewMotion();
            leadingIndent = 0;
            discardedHeight = 0;
        } else if (removed > 0) {
            // Carry the old line's indentation across the bounded prefix cut.
            // Discarded rows change the local origin, never the visible reading position.
            prefixMeasure.indent = leadingIndent;
            prefixMeasure.text = previous.slice(0, removed).join("") + "\u200b";
            prefixMeasure.forceLayout();
            let indent = prefixMeasure.lastEnd;
            let rows = prefixMeasure.lastRow;
            prefixMeasure.text = "";
            const nextWord = preview.split(" ", 1)[0];
            const nextWidth = previewMetrics.advanceWidth(nextWord);
            if (indent >= transcript.width - 0.1
                || (nextWidth <= transcript.width && nextWidth > transcript.width - indent)) {
                indent = 0;
                rows++;
            }
            leadingIndent = indent;
            discardedHeight += rows * lineHeight;
        }
        displayedStart = previewStart;
        displayedIdentifier = identifier;
        displayedPreview = preview;
        transcript.forceLayout();
    }
    function resetPreviewLayout() {
        if (!previewReady) return;
        displayedPreview = "";
        Qt.callLater(syncPreview);
    }
    onWidthChanged: resetPreviewLayout()
    onTextSizeChanged: resetPreviewLayout()
    onPreviewChanged: Qt.callLater(syncPreview)
    onPreviewStartChanged: Qt.callLater(syncPreview)
    onPhaseChanged: {
        if (!reviewing || busy) menuOpen = false;
        resetCountdown();
        resetPreviewMotion();
    }
    onIdentifierChanged: { menuOpen = false; resetCountdown(); resetPreviewMotion(); Qt.callLater(syncPreview); }
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
        previewReady = true;
        syncPreview();
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
        objectName: "overlay-surface"
        anchors.fill: parent
        color: Qt.alpha(Color.popups.background, root.surfaceOpacity)
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
                objectName: "recording-header"
                width: parent.width
                visible: !root.reviewing
                spacing: 8
                RecordingLight {
                    id: recordingDot
                    objectName: "recording-dot"
                    active: root.phase === "recording"
                    animate: root.smoothScrolling && root.scrollDuration > 0
                    ink: root.emphasis
                    Accessible.name: root.status
                }
                Text {
                    objectName: "recording-status"
                    Layout.fillWidth: true
                    text: root.phase === "recording" ? "" : root.status
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: Style.font.body
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Text {
                    objectName: "recording-timer"
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
                height: root.lineHeight * root.previewLines
                visible: root.preview.length > 0 || root.phase === "recording"
                clip: true
                Item {
                    id: previewMotion
                    property real offset: Math.min(0, transcriptViewport.height - transcript.height
                        - root.discardedHeight - transcript.lookAhead)
                    Behavior on offset {
                        enabled: root.animatePreview && root.visible && root.smoothScrolling && root.scrollDuration > 0
                        SmoothedAnimation {
                            velocity: -1
                            duration: root.scrollDuration
                            maximumEasingTime: -1
                            reversingMode: SmoothedAnimation.Eased
                        }
                    }
                }
                // Start making room near the end of the last visible line, then
                // follow wraps smoothly instead of shifting a whole line at once.
                Text {
                    id: transcript
                    objectName: "transcript-text"
                    property real lastLineFill: 0
                    readonly property real lookAhead: (root.phase === "recording" || root.busy)
                        && lineCount >= root.previewLines
                        ? root.lineHeight * Math.min(1.5, root.scrollLookahead * 0.325) * Math.max(0, Math.min(1, (lastLineFill - 0.72) / 0.28)) : 0
                    width: parent.width
                    // A corrected/committed preview can be shorter than the
                    // preceding partial. Never paint above its new tail while
                    // the old scroll animation is still catching up.
                    y: Math.max(previewMotion.offset + root.discardedHeight,
                        Math.min(0, transcriptViewport.height - height - lookAhead))
                    text: root.displayedPreview
                    color: Color.popups.text
                    font.family: Style.font.family
                    font.pixelSize: root.textSize
                    onFontChanged: root.resetPreviewLayout()
                    wrapMode: Text.Wrap
                    lineHeightMode: Text.FixedHeight
                    lineHeight: root.lineHeight
                    textFormat: Text.PlainText
                    onLineLaidOut: line => {
                        if (line.number === 0 && root.leadingIndent > 0) {
                            line.x = effectiveHorizontalAlignment === Text.AlignRight ? 0 : root.leadingIndent;
                            line.width = width - root.leadingIndent;
                        }
                        if (line.isLast) lastLineFill = line.implicitWidth / Math.max(1, line.width);
                    }
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
                    iconText: "⧉"
                    tooltipText: "Copy"
                    Accessible.name: "Copy"
                    visible: root.showCopy
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
                    tooltipText: root.busy ? "Dismiss" : "Dismiss · closes after " + root.reviewDuration / 1000 + " idle seconds"
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
    FontMetrics { id: previewMetrics; font: transcript.font }
    Text {
        id: prefixMeasure
        visible: false
        property real indent: 0
        property real lastEnd: 0
        property int lastRow: 0
        width: transcript.width
        font: transcript.font
        wrapMode: Text.Wrap
        lineHeightMode: Text.FixedHeight
        lineHeight: root.lineHeight
        textFormat: Text.PlainText
        onLineLaidOut: line => {
            if (line.number === 0 && indent > 0) {
                line.x = effectiveHorizontalAlignment === Text.AlignRight ? 0 : indent;
                line.width = width - indent;
            }
            if (line.isLast) { lastEnd = (line.number === 0 ? indent : 0) + line.implicitWidth; lastRow = line.number; }
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
            color: Qt.alpha(Color.popups.background, 0.94)
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
