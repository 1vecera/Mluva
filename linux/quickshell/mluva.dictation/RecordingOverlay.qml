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
    readonly property bool hyprlandSession: !!Quickshell.env("HYPRLAND_INSTANCE_SIGNATURE")
    property bool windowRulesReady: !hyprlandSession
    property bool smoothScrolling: true
    property int scrollDuration: 800
    property int scrollLookahead: 2
    property string positionPreset: "bottom-center"
    property bool repositionRequested: false
    property bool userPlaced: false
    readonly property string horizontalRule: positionPreset === "bottom-left" ? "24"
        : positionPreset === "bottom-right" ? "(monitor_w-window_w)-24" : "(monitor_w-window_w)/2"
    property bool dismissed: false
    readonly property bool active: ["preparing", "recording", "processing", "error"].includes(phase)
    readonly property color emphasis: phase === "recording" || phase === "error"
        ? Color.urgent : Color.accent
    readonly property int textSize: 14
    readonly property int lineHeight: 22
    property int previewLines: 5
    property real surfaceOpacity: 0.82
    property bool animatePreview: false
    property bool previewReady: false
    property string displayedPreview: ""
    property int displayedStart: 0
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
        const continuous = displayedPreview.length > 0
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
        if (!active) resetPreviewLayout();
        dismissed = false;
        resetPreviewMotion();
    }
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
    onClosed: dismissed = true
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

    NumberAnimation {
        id: revisionAnimation
        target: root
        property: "revisionProgress"
        from: 0
        to: 1
        duration: 180
        easing.type: Easing.OutCubic
    }

    Item {
        id: surface
        anchors.fill: parent
        Keys.onEscapePressed: root.dismissed = true
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
                    text: root.phase === "recording" ? "" : root.status
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
            visible: root.preview.length > 0 || root.phase === "recording"
            implicitHeight: 20 + (transcriptViewport.visible ? root.lineHeight * root.previewLines : 0)
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
}
