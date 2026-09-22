import QtQuick
import QtQuick.Layouts
import Quickshell
import Quickshell.Io
import Quickshell.Hyprland
import qs.Commons
import qs.Ui
import "TextMotion.js" as TextMotion

FloatingWindow {
    id: root
    FontLoader { id: mono; source: "fonts/JetBrainsMono-Regular.ttf" }
    objectName: "mluva-recording-overlay"
    required property string phase
    required property int elapsed
    required property real level
    required property string preview
    property int previewStart: 0
    property string identifier: ""
    property var options: []
    property string message: ""
    readonly property bool hyprlandSession: !!Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE")
    property bool windowRulesReady: !hyprlandSession
    property bool menuOpen: false
    property int reviewDuration: 4000
    property bool showCopy: true
    property bool smoothScrolling: true
    property int scrollDuration: 800
    property int scrollLookahead: 2
    property string positionPreset: "bottom-center"
    property bool repositionRequested: false
    property bool userPlaced: false
    readonly property string horizontalRule: positionPreset === "bottom-left" ? "24"
        : positionPreset === "bottom-right" ? "(monitor_w-window_w)-24" : "(monitor_w-window_w)/2"
    property real remaining: reviewDuration
    property double lastTick: Date.now()
    property bool dismissed: false
    property bool reviewInteracted: false
    readonly property bool countdownPaused: reviewHover.hovered || menuOpen || (reviewInteracted && surface.Window.active)
    readonly property bool reviewing: ["ready", "rewriting", "review-error"].includes(phase)
    readonly property bool busy: phase === "rewriting"
    readonly property bool active: reviewing || ["preparing", "recording", "processing", "error"].includes(phase)
    readonly property color emphasis: phase === "recording" || phase === "error" || phase === "review-error"
        ? Color.urgent : Color.accent
    readonly property int textSize: 14
    readonly property int lineHeight: 22
    property bool rewriteEnabled: true
    property int previewLines: 5
    property real surfaceOpacity: 0.82
    property bool animatePreview: false
    property bool previewReady: false
    property string displayedPreview: ""
    property int displayedStart: 0
    property string displayedIdentifier: ""
    property real leadingIndent: 0
    property real discardedHeight: 0
    property real scrollTarget: 0
    property real virtualTail: 0
    readonly property real revisionInset: Math.max(0, virtualTail - discardedHeight - transcript.height)
    property bool following: true
    property var speechSamples: []
    property int speechHighWater: 0
    property var revisionChanges: []
    property real revisionProgress: 1
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
            && removed >= 0 && removed < previous.length && (removed === 0 || preview.startsWith(overlap));
        if (!continuous) {
            resetPreviewMotion();
            leadingIndent = 0;
            discardedHeight = 0;
            scrollTarget = 0;
            virtualTail = 0;
            following = true;
            speechSamples = [];
            speechHighWater = 0;
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
        revisionAnimation.stop();
        // New speech is immediate. Only an existing-word correction may settle
        // by three pixels, always at full opacity and without delaying the text.
        revisionChanges = removed === 0 && continuous && !preview.startsWith(displayedPreview)
            ? TextMotion.changes(displayedPreview, preview) : [];
        displayedPreview = preview;
        revisionProgress = 1;
        if (revisionChanges.length && smoothScrolling && scrollDuration > 0 && root.visible) {
            revisionProgress = 0;
            revisionAnimation.start();
        }
        const now = Date.now();
        speechHighWater = Math.max(speechHighWater, previewStart + Array.from(preview).length);
        speechSamples = speechSamples.filter(sample => sample[0] > now - 4000).concat([[now, speechHighWater]]).slice(-128);
        transcript.forceLayout();
        Qt.callLater(updateScrollTarget);
    }
    function updateScrollTarget() {
        virtualTail = Math.max(virtualTail, transcript.height + discardedHeight);
        if (following) scrollTarget = Math.min(scrollTarget, 0,
            transcriptViewport.height - virtualTail - transcript.lookAhead);
    }
    function resetPreviewLayout() {
        if (!previewReady) return;
        displayedPreview = "";
        Qt.callLater(syncPreview);
    }
    function scrollPreview(delta) {
        // Rebase retained correction space only when the reader takes control.
        // Keep the current painted origin, then clamp to the available buffer.
        if (revisionInset > 0) {
            const origin = previewMotion.offset + revisionInset;
            resetPreviewMotion();
            virtualTail = transcript.height + discardedHeight;
            scrollTarget = origin;
        }
        const head = -discardedHeight;
        const tail = Math.min(head, transcriptViewport.height - virtualTail);
        scrollTarget = Math.max(tail, Math.min(head, scrollTarget + delta));
        following = delta < 0 && scrollTarget <= tail + 1;
    }
    onWidthChanged: resetPreviewLayout()
    onHeightChanged: if (!userPlaced) Qt.callLater(applyPosition)
    onActiveChanged: {
        if (active) { userPlaced = false; Qt.callLater(applyPosition); }
    }
    onPositionPresetChanged: {
        userPlaced = false;
        if (hyprlandSession) { repositionRequested = true; windowRules.running = true; }
        else applyPosition();
    }
    function applyPosition() {
        // Explicit presets move only this surface. Ordinary frames never undo a manual drag.
        if (!screen || !contentItem.Window.window) return;
        const margin = 24;
        const x = positionPreset === "bottom-left" ? margin
            : positionPreset === "bottom-right" ? Math.max(margin, screen.width - width - margin)
            : Math.max(margin, (screen.width - width) / 2);
        const y = Math.max(margin, screen.height - height - 48);
        if (!hyprlandSession) {
            contentItem.Window.window.x = screen.x + x;
            contentItem.Window.window.y = screen.y + y;
            return;
        }
        const window = Hyprland.toplevels.values.find(item => item.title === root.title
            && item.wayland?.appId === "org.quickshell");
        if (window) Hyprland.dispatch("hl.dsp.window.move({window='address:0x" + window.address
            + "',x=" + Math.round(screen.x + x) + ",y=" + Math.round(screen.y + y) + ",relative=false})");
    }
    onTextSizeChanged: resetPreviewLayout()
    onPreviewLinesChanged: {
        resetPreviewLayout();
        Qt.callLater(() => {
            const window = contentItem.Window.window;
            if (window) window.height = root.implicitHeight;
        });
    }
    onPreviewChanged: Qt.callLater(syncPreview)
    onPreviewStartChanged: Qt.callLater(syncPreview)
    onPhaseChanged: {
        reviewInteracted = false;
        if (!active) resetPreviewLayout();
        if (!reviewing || busy) menuOpen = false;
        resetCountdown();
        resetPreviewMotion();
    }
    onIdentifierChanged: {
        reviewInteracted = false; menuOpen = false;
        resetCountdown(); resetPreviewMotion(); Qt.callLater(syncPreview);
    }
    onMessageChanged: resetCountdown()
    onCountdownPausedChanged: lastTick = Date.now()
    title: "Mluva recording"
    visible: active && !dismissed && windowRulesReady
    implicitWidth: Math.min(500, screen ? screen.width - 32 : 500)
    implicitHeight: recordingHeader.implicitHeight + (transcriptSurface.visible ? transcriptSurface.implicitHeight : 0)
    minimumSize: Qt.size(Math.min(320, implicitWidth), implicitHeight)
    color: "transparent"
    // Keep native window controls (including Super+T) and let the compositor
    // own position and size after mapping. Opening must not steal dictation focus.
    onWindowConnected: {
        contentItem.Window.window.flags = Qt.Window | Qt.FramelessWindowHint | Qt.WindowStaysOnTopHint;
        if (!hyprlandSession) Qt.callLater(applyPosition);
    }
    onClosed: { dismissed = true; menuOpen = false; }
    Component.onCompleted: {
        previewReady = true;
        syncPreview();
    }
    Process {
        id: windowRules
        running: root.hyprlandSession
        // Replace the active named rule when the plugin reloads.
        // Float and pin are initial rules; they never override the user's tiling.
        command: ["hyprctl", "eval",
            "if mluva_recording_rule then mluva_recording_rule:set_enabled(false) end; "
            + "mluva_recording_rule = hl.window_rule({match = {class = 'org.quickshell', title = 'Mluva recording'}, "
            + "float = true, pin = true, no_initial_focus = true, no_follow_mouse = true, decorate = false, "
            + "move = {'" + root.horizontalRule + "', '(monitor_h-window_h)-48'}})"]
        onExited: exitCode => {
            root.windowRulesReady = exitCode === 0;
            if (exitCode === 0 && root.repositionRequested) {
                root.repositionRequested = false;
                root.applyPosition();
            }
            if (exitCode !== 0) console.warn("Mluva recording window rules could not be installed");
        }
    }
    Connections {
        target: Hyprland
        function onRawEvent(event) {
            if (event.name === "configreloaded" && root.hyprlandSession) windowRules.running = true;
            if (event.name !== "changefloatingmode") return;
            const [address, floating] = event.parse(2);
            const window = Hyprland.toplevels.values.find(item => item.address === address
                && item.title === root.title && item.wayland?.appId === "org.quickshell");
            if (window) root.userPlaced = true;
            if (floating !== "1") return;
            // Tiling clears Hyprland's pin. Restore it only when this recorder
            // returns to floating; setting (rather than toggling) is idempotent.
            if (window) Hyprland.dispatch("hl.dsp.window.pin({action = 'set', window = 'address:0x" + window.address + "'})");
        }
    }

    component ActionButton: Button {
        opacity: enabled ? 1 : 0.4
        foreground: Color.popups.text
        fontSize: root.textSize
        horizontalPadding: 8
        verticalPadding: 5
        focusable: true
        Keys.onPressed: event => { if (root.reviewing) root.reviewInteracted = true; }
    }

    NumberAnimation {
        id: revisionAnimation
        target: root
        property: "revisionProgress"
        from: 0
        to: 1
        duration: 180
        easing.type: Easing.OutCubic
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

    Item {
        id: surface
        anchors.fill: parent
        Keys.onPressed: event => { if (root.reviewing) root.reviewInteracted = true; }
        Keys.onEscapePressed: root.menuOpen ? root.menuOpen = false : root.act("dismiss")
        HoverHandler { id: reviewHover }
        TapHandler {
            onPressedChanged: if (pressed && root.reviewing) root.reviewInteracted = true
        }
        // The preview and its bare status row share one native move target. Buttons
        // painted above this area retain their own pointer handling.
        MouseArea {
            objectName: "recorder-drag-area"
            anchors.fill: parent
            cursorShape: Qt.SizeAllCursor
            onPressed: { root.userPlaced = true; root.startSystemMove(); }
        }
        Item {
            id: recordingHeader
            objectName: "recording-header"
            anchors.top: parent.top
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.leftMargin: -2
            anchors.rightMargin: 0
            implicitHeight: statusRow.implicitHeight
            RowLayout {
                id: statusRow
                anchors.fill: parent
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
                    text: root.reviewing ? "Review" : root.phase === "recording" ? "" : root.status
                    color: Color.popups.text
                    font.family: mono.name
                    font.pixelSize: root.textSize
                    textFormat: Text.PlainText
                    elide: Text.ElideRight
                }
                Text {
                    objectName: "recording-timer"
                    visible: root.phase === "recording"
                    text: root.timer
                    color: Color.popups.text
                    font.family: mono.name
                    font.pixelSize: root.textSize
                    Layout.alignment: Qt.AlignBottom
                }
            }
        }
        BorderSurface {
            id: transcriptSurface
            objectName: "overlay-surface"
            anchors.left: parent.left
            anchors.right: parent.right
            anchors.top: recordingHeader.bottom
            anchors.topMargin: 0
            anchors.bottom: parent.bottom
            visible: root.preview.length > 0 || root.phase === "recording" || root.reviewing
            implicitHeight: 20 + (transcriptViewport.visible ? root.lineHeight * root.previewLines : 0)
                + (reviewMessage.visible ? reviewMessage.implicitHeight + 6 : 0)
                + (reviewActions.visible ? reviewActions.implicitHeight + 6 : 0)
            color: Qt.alpha(Color.popups.background, root.surfaceOpacity)
            radius: Style.cornerRadius
            borderSpec: Border.surfaceSpec("popups", "border", Color.popups.border, 1)
            Column {
                id: body
                anchors.left: parent.left
                anchors.right: parent.right
                anchors.top: parent.top
                anchors.margins: 10
                spacing: 6
                Item {
                    id: transcriptViewport
                    objectName: "transcript-viewport"
                    width: parent.width
                    height: root.lineHeight * root.previewLines
                    visible: root.preview.length > 0 || root.phase === "recording"
                    clip: true
                    Item {
                        id: previewMotion
                        property real offset: root.scrollTarget
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
                    WheelHandler {
                        target: null
                        onWheel: event => {
                            root.scrollPreview(event.angleDelta.y / 3);
                            event.accepted = true;
                        }
                    }
                    Text {
                        visible: root.revisionProgress < 1
                        width: parent.width
                        y: transcript.y + 3 * (1 - root.revisionProgress)
                        text: TextMotion.styled(root.displayedPreview, root.revisionChanges, false,
                            1, Color.popups.text, true)
                        font: transcript.font
                        wrapMode: Text.Wrap
                        lineHeightMode: Text.FixedHeight
                        lineHeight: root.lineHeight
                        textFormat: Text.StyledText
                        onLineLaidOut: line => {
                            if (line.number === 0 && root.leadingIndent > 0) {
                                line.x = effectiveHorizontalAlignment === Text.AlignRight ? 0 : root.leadingIndent;
                                line.width = width - root.leadingIndent;
                            }
                        }
                    }
                    Text {
                        id: transcript
                        objectName: "transcript-text"
                        property real lastLineFill: 0
                        readonly property real lookAhead: (root.phase === "recording" || root.busy)
                            && lineCount >= root.previewLines
                            ? root.lineHeight * TextMotion.forecast(root.speechSamples,
                                width / Math.max(1, previewMetrics.advanceWidth("M")), lastLineFill,
                                root.scrollDuration / 1000 + 0.25, root.scrollLookahead) : 0
                        width: parent.width
                        // A corrected/committed preview can be shorter than the
                        // preceding partial. Never paint above its new tail while
                        // the old scroll animation is still catching up.
                        y: previewMotion.offset + root.discardedHeight + root.revisionInset
                        text: root.revisionProgress < 1
                            ? TextMotion.styled(root.displayedPreview, root.revisionChanges, false,
                                0, Color.popups.text) : root.displayedPreview
                        color: Color.popups.text
                        font.family: mono.name
                        font.pixelSize: root.textSize
                        onFontChanged: root.resetPreviewLayout()
                        wrapMode: Text.Wrap
                        lineHeightMode: Text.FixedHeight
                        lineHeight: root.lineHeight
                        textFormat: root.revisionProgress < 1 ? Text.StyledText : Text.PlainText
                        onHeightChanged: Qt.callLater(root.updateScrollTarget)
                        onLookAheadChanged: Qt.callLater(root.updateScrollTarget)
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
                    id: reviewMessage
                    width: parent.width
                    visible: root.reviewing && root.message.length > 0
                    text: root.message
                    color: root.phase === "review-error" ? Color.urgent : Color.popups.text
                    font.family: mono.name
                    font.pixelSize: root.textSize
                    textFormat: Text.PlainText
                    wrapMode: Text.Wrap
                }
                RowLayout {
                    id: reviewActions
                    width: parent.width
                    visible: root.reviewing
                    spacing: 2
                    ActionButton {
                        objectName: "continue-button"
                        text: "Continue"
                        tooltipText: "Continue recording in this conversation"
                        Accessible.name: "Continue recording"
                        visible: !root.busy
                        onClicked: root.act("continue")
                    }
                    ActionButton {
                        objectName: "polish-button"
                        text: "Polish"
                        enabled: root.rewriteEnabled
                        visible: !root.busy
                        onClicked: root.act("rewrite", "polish")
                    }
                    ActionButton {
                        objectName: "structure-button"
                        text: "Structure"
                        enabled: root.rewriteEnabled
                        visible: !root.busy
                        onClicked: root.act("rewrite", "structure")
                    }
                    ActionButton {
                        id: more
                        objectName: "more-button"
                        text: "More ▴"
                        visible: !root.busy
                        enabled: root.rewriteEnabled && root.options.length > 0
                        selected: root.menuOpen
                        onClicked: root.menuOpen = !root.menuOpen
                    }
                    Text {
                        visible: root.busy
                        text: "Rewriting…"
                        color: Color.popups.text
                        font.family: mono.name
                        font.pixelSize: root.textSize
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
                        font.family: mono.name
                        font.pixelSize: root.textSize
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
