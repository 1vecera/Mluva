import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons

Item {
    id: root
    FontLoader { id: mono; source: "fonts/JetBrainsMono-Regular.ttf" }
    property var bar
    property string moduleName: "mluva.dictation"
    property var settings
    property string phase: "unavailable"
    property int elapsed: 0
    property real level: 0
    property string preview: ""
    property int previewStart: 0
    property bool reduceMotion: false
    property int widgetLines: 5
    property real widgetOpacity: 0.82
    property string widgetPosition: "bottom-center"
    property bool controlFailed: false
    // Omarchy creates one widget per monitor. Only the first attached widget
    // owns the floating recorder; removing its bar hands ownership to the next.
    readonly property bool ownsOverlay: !bar || !bar.moduleSlots
        || bar.moduleSlots.find(slot => slot.moduleName === root.moduleName && slot.activeItem)?.activeItem === root
    readonly property string executable: settings && settings.command ? settings.command : "mluva-shell"
    readonly property var labels: ({
        "unavailable": "Mluva unavailable", "stopped": "Mluva stopped", "idle": "Mluva ready",
        "preparing": "Preparing", "recording": "REC " + elapsed + "s",
        "processing": "Transcribing", "error": "Mluva error"
    })
    readonly property string tooltip: controlFailed ? "Control failed. Start Mluva and inspect its setup status." :
        (labels[phase] + ". Left: start/stop (clipboard only). Right: cancel. Middle: open latest."
        + (phase === "error" ? " Open Mluva for error details." : ""))
    implicitWidth: bar && bar.vertical ? bar.barSize : content.implicitWidth + 16
    implicitHeight: bar ? bar.barSize : 26
    activeFocusOnTab: true
    Accessible.role: Accessible.Button
    Accessible.name: phase === "recording" ? "Recording · " + elapsed + " seconds · Stop dictation" : labels[phase]
    Accessible.description: tooltip
    Accessible.onPressAction: if (phase !== "processing") control("record")
    Keys.onSpacePressed: if (phase !== "processing") control("record")
    Keys.onReturnPressed: if (phase !== "processing") control("record")
    Keys.onEscapePressed: control("cancel")

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
                    root.previewStart = Number.isInteger(state.preview_start)
                        ? Math.max(0, Math.min(2147483647, state.preview_start)) : 0;
                    // The bridge bounds Unicode characters before serializing this string.
                    root.preview = typeof state.preview === "string" ? state.preview : "";

                    root.reduceMotion = state.reduce_motion === true;
                    root.widgetLines = Math.max(1, Math.min(10, state.widget_lines ?? 5));
                    root.widgetOpacity = Math.max(0.1, Math.min(1, (state.widget_opacity ?? 82) / 100));

                    root.widgetPosition = ["bottom-left", "bottom-center", "bottom-right"].includes(state.widget_position)
                        ? state.widget_position : "bottom-center";

                } catch (error) {
                    root.phase = "unavailable";
                    root.elapsed = 0;
                    root.level = 0;
                    root.preview = "";
                    root.previewStart = 0;

                }
            }
        }
        onExited: {
            root.phase = "unavailable";
            root.elapsed = 0;
            root.level = 0;
            root.preview = "";
            root.previewStart = 0;

        }
    }
    LazyLoader {
        objectName: "mluva-overlay-loader"
        active: root.ownsOverlay
        RecordingOverlay {
            smoothScrolling: !root.reduceMotion
            positionPreset: root.widgetPosition
            previewLines: root.widgetLines
            surfaceOpacity: root.widgetOpacity
            phase: root.phase
            elapsed: root.elapsed
            level: root.level
            preview: root.preview
            previewStart: root.previewStart
        }
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
    Row {
        id: content
        anchors.centerIn: parent
        spacing: 4
        RecordingLight {
            anchors.verticalCenter: parent.verticalCenter
            visible: root.phase === "recording"
            active: root.phase === "recording"
            animate: true
            ink: Color.urgent
            Accessible.ignored: true
        }
        Text {
            id: label
            anchors.verticalCenter: parent.verticalCenter
            visible: !(root.bar && root.bar.vertical && root.phase === "recording")
            text: root.bar && root.bar.vertical ? "M" : root.phase === "recording"
                ? Math.floor(root.elapsed / 60).toString().padStart(2, "0")
                    + ":" + (root.elapsed % 60).toString().padStart(2, "0") : root.labels[root.phase]
            color: root.phase === "error" || root.controlFailed ? Color.urgent
                : (root.bar ? root.bar.foreground : Color.foreground)
            font.family: mono.name
            font.pixelSize: root.bar && root.bar.vertical ? 10 : 12
            textFormat: Text.PlainText
        }
    }
    Rectangle {
        anchors.fill: parent
        color: "transparent"
        border.color: Color.accent
        radius: 2
        visible: root.activeFocus
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
