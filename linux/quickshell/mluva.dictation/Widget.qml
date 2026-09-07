import QtQuick
import Quickshell
import Quickshell.Io

Item {
    id: root
    property var bar
    property string moduleName: "mluva.dictation"
    property var settings
    property string phase: "unavailable"
    property int elapsed: 0
    property real level: 0
    property string preview: ""
    property bool controlFailed: false
    readonly property string executable: settings && settings.command ? settings.command : "mluva-shell"
    readonly property var labels: ({
        "unavailable": "Mluva unavailable", "stopped": "Mluva stopped", "idle": "Mluva ready",
        "preparing": "Preparing", "recording": "REC " + elapsed + "s",
        "processing": "Transcribing", "copied": "Copied", "error": "Mluva error"
    })
    readonly property string tooltip: controlFailed ? "Control failed. Start Mluva and inspect its setup status." :
        (labels[phase] + ". Left: start/stop (clipboard only). Right: cancel. Middle: open latest."
        + (phase === "error" ? " Open Mluva for error details." : ""))
    implicitWidth: bar && bar.vertical ? bar.barSize : (label ? label.implicitWidth + 16 : 26)
    implicitHeight: bar ? bar.barSize : 26
    SystemPalette { id: palette }

    function control(action) {
        if (controlProcess.running) return;
        controlFailed = false;
        controlProcess.command = [executable, action];
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
                    root.preview = typeof state.preview === "string" ? state.preview.slice(-180) : "";
                } catch (error) {
                    root.phase = "unavailable";
                    root.elapsed = 0;
                    root.level = 0;
                    root.preview = "";
                }
            }
        }
        onExited: {
            root.phase = "unavailable";
            root.elapsed = 0;
            root.level = 0;
            root.preview = "";
        }
    }
    RecordingOverlay {
        screen: root.QsWindow.window ? root.QsWindow.window.screen : null
        bar: root.bar
        phase: root.phase
        elapsed: root.elapsed
        level: root.level
        preview: root.preview
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
            (root.bar ? root.bar.urgent : palette.highlight) : (root.bar ? root.bar.foreground : palette.windowText)
        font.family: root.bar ? root.bar.fontFamily : "monospace"
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
