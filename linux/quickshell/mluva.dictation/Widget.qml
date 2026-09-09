import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

Item {
    id: root
    property var bar
    property string moduleName: "mluva.dictation"
    property var settings
    property string phase: "unavailable"
    property int elapsed: 0
    property real level: 0
    property string preview: ""
    property int previewStart: 0
    property string identifier: ""
    property var options: []
    property string message: ""
    property int reviewTimeout: 4
    property bool showCopy: true
    property bool smoothScrolling: true
    property int scrollDuration: 800
    property int scrollLookahead: 2
    property bool controlFailed: false
    readonly property string executable: settings && settings.command ? settings.command : "mluva-shell"
    readonly property var labels: ({
        "unavailable": "Mluva unavailable", "stopped": "Mluva stopped", "idle": "Mluva ready",
        "preparing": "Preparing", "recording": "REC " + elapsed + "s",
        "processing": "Transcribing", "ready": "Mluva", "rewriting": "Rewriting",
        "review-error": "Rewrite failed", "error": "Mluva error"
    })
    readonly property string tooltip: controlFailed ? "Control failed. Start Mluva and inspect its setup status." :
        (labels[phase] + ". Left: start/stop (clipboard only). Right: cancel. Middle: open latest."
        + (phase === "error" ? " Open Mluva for error details." : ""))
    implicitWidth: bar && bar.vertical ? bar.barSize : (label ? label.implicitWidth + 16 : 26)
    implicitHeight: bar ? bar.barSize : 26

    function control(action) {
        if (controlProcess.running) return;
        controlFailed = false;
        controlProcess.command = [executable, action];
        controlProcess.running = true;
    }
    function review(action, style) {
        if (controlProcess.running || !identifier) return;
        controlFailed = false;
        controlProcess.command = [executable, "review", action, identifier, style];
        controlProcess.running = true;
    }

    Process {
        id: watcher
        command: [root.executable, "watch", "--overlay"]
        running: true
        stdout: SplitParser {
            onRead: function(data) {
                try {
                    const state = JSON.parse(data);
                    root.phase = Object.prototype.hasOwnProperty.call(root.labels, state.phase) ? state.phase : "unavailable";
                    root.elapsed = Number.isInteger(state.elapsed) ? Math.max(0, Math.min(86400, state.elapsed)) : 0;
                    root.level = Number.isFinite(state.level) ? Math.max(0, Math.min(1, state.level)) : 0;
                    root.previewStart = Number.isInteger(state.preview_start)
                        ? Math.max(0, Math.min(2147483647, state.preview_start)) : 0;
                    // The bridge bounds Unicode characters before serializing this string.
                    root.preview = typeof state.preview === "string" ? state.preview : "";
                    root.identifier = typeof state.identifier === "string" ? state.identifier.slice(0, 36) : "";
                    root.options = Array.isArray(state.options) ? state.options.slice(0, 128) : [];
                    root.reviewTimeout = Number.isInteger(state.review_timeout) ? Math.max(1, Math.min(60, state.review_timeout)) : 4;
                    root.showCopy = state.show_copy !== false;
                    root.smoothScrolling = state.smooth_scrolling !== false;
                    root.scrollDuration = Number.isInteger(state.scroll_duration) ? state.scroll_duration : 800;
                    root.scrollLookahead = Number.isInteger(state.scroll_lookahead) ? state.scroll_lookahead : 2;
                    root.message = typeof state.message === "string" ? state.message.slice(0, 96) : "";
                } catch (error) {
                    root.phase = "unavailable";
                    root.elapsed = 0;
                    root.level = 0;
                    root.preview = "";
                    root.previewStart = 0;
                    root.identifier = "";
                    root.options = [];
                    root.message = "";
                }
            }
        }
        onExited: {
            root.phase = "unavailable";
            root.elapsed = 0;
            root.level = 0;
            root.preview = "";
            root.previewStart = 0;
            root.identifier = "";
            root.options = [];
            root.message = "";
        }
    }
    RecordingOverlay {
        screen: root.QsWindow.window ? root.QsWindow.window.screen : null
        bar: root.bar
        reviewDuration: root.reviewTimeout * 1000
        showCopy: root.showCopy
        smoothScrolling: root.smoothScrolling
        scrollDuration: root.scrollDuration
        scrollLookahead: root.scrollLookahead
        phase: root.phase
        elapsed: root.elapsed
        level: root.level
        preview: root.preview
        previewStart: root.previewStart
        identifier: root.identifier
        options: root.options
        message: root.controlFailed ? "Control failed · open Mluva" : root.message
        onReview: function(action, style) { root.review(action, style); }
    }
    Timer {
        interval: 5000
        repeat: true
        running: true
        onTriggered: if (!watcher.running) watcher.running = true
    }
    Process {
        id: controlProcess
        onExited: function(exitCode) { root.controlFailed = exitCode !== 0; }
    }
    Text {
        id: label
        anchors.centerIn: parent
        text: root.bar && root.bar.vertical ? (root.phase === "recording" ? "REC" : "M") : root.labels[root.phase]
        color: root.phase === "recording" || root.phase === "error" || root.controlFailed ?
            Color.urgent : (root.bar ? root.bar.foreground : Color.foreground)
        font.family: Style.font.family
        font.pixelSize: root.bar && root.bar.vertical ? 10 : 12
        textFormat: Text.PlainText
    }
    MouseArea {
        anchors.fill: parent
        hoverEnabled: true
        acceptedButtons: Qt.LeftButton | Qt.RightButton | Qt.MiddleButton
        onEntered: if (root.bar) root.bar.showTooltip(root, root.tooltip)
        onExited: if (root.bar) root.bar.hideTooltip(root)
        onClicked: function(mouse) {
            if (mouse.button === Qt.RightButton) root.control("cancel");
            else if (mouse.button === Qt.MiddleButton) root.control("latest");
            else if (root.phase !== "processing") root.control("record");
        }
    }
}
