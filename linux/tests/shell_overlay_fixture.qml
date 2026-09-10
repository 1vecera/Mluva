import QtQuick
import Quickshell
import Quickshell.Io
import qs.Commons
import "../quickshell/mluva.dictation" as Mluva

ShellRoot {
    id: root
    readonly property var overlay: widget.data.find(item => item.objectName === "mluva-recording-overlay")
    function descendants(item): var {
        return [item].concat(...(item.children || []).map(child => descendants(child)));
    }
    IpcHandler {
        target: "fixture"
        function click(name: string): void {
            if (name === "more") root.overlay.menuOpen = true;
            else root.descendants(root.overlay.contentItem).find(item => item.objectName === name + "-button").clicked();
        }
        function theme(light: bool): void {
            Color.loadColors(light ? 'background = "#faf4ed"\nforeground = "#575279"\naccent = "#286983"\nred = "#b4637a"'
                : 'background = "#1a1b26"\nforeground = "#c0caf5"\naccent = "#7aa2f7"\nred = "#f7768e"');
        }
        function option(index: int): void {
            const menu = root.overlay.data.find(item => item.objectName === "rewrite-menu");
            root.descendants(menu.contentItem).find(item => item.objectName === "rewrite-option-" + index).clicked();
        }
        function countdown(): string {
            const origin = root.overlay.contentItem.mapToGlobal(0, 0);
            return JSON.stringify({remaining: root.overlay.remaining, paused: root.overlay.countdownPaused,
                visible: root.overlay.visible, x: origin.x, y: origin.y});
        }
        function focusReview(): void {
            root.descendants(root.overlay.contentItem).find(item => item.objectName === "polish-button").forceActiveFocus();
            root.overlay.contentItem.Window.window.requestActivate();
        }
        function closeMenu(): void { root.overlay.menuOpen = false; }
        function motionSamples(): string {
            let shortText = "", nearEdge = "", wrapped = "";
            for (let count = 1; count < 200; count++) {
                probe.text = ("Steady words for a readable preview. ".repeat(40)).split(" ").slice(0, count).join(" ");
                probe.forceLayout();
                if (probe.lineCount < 5) shortText = probe.text;
                if (probe.lineCount === 5 && probe.lastFill > 0.8) nearEdge = probe.text;
                if (probe.lineCount === 6 && nearEdge) { wrapped = probe.text; break; }
            }
            return JSON.stringify({shortText, nearEdge, wrapped});
        }
        function expectedTail(value: string): string {
            probe.text = value;
            probe.forceLayout();
            return JSON.stringify({lastFill: probe.lastFill, lineCount: probe.lineCount});
        }
    }
    Text {
        id: probe
        visible: false
        width: root.overlay ? root.overlay.width - 20 : 480
        font.family: Style.font.family
        font.pixelSize: root.overlay ? root.overlay.textSize : 12
        wrapMode: Text.Wrap
        lineHeightMode: Text.FixedHeight
        lineHeight: root.overlay ? root.overlay.lineHeight : 17
        textFormat: Text.PlainText
        property real lastFill: 0
        onLineLaidOut: line => { if (line.isLast) lastFill = line.implicitWidth / Math.max(1, line.width); }
    }
    FloatingWindow {
        id: editor
        title: "Mluva overlay fixture"
        screen: Quickshell.screens[0]
        visible: true
        implicitWidth: 300
        implicitHeight: 100
        TextInput {
            id: input
            text: "Synthetic editor keeps keyboard focus"
            anchors.centerIn: parent
            focus: true
        }
        Mluva.Widget {
            id: widget
            settings: ({command: Quickshell.env("MLUVA_SHELL_COMMAND")})
        }
    }
    function snapshot(): string {
            const overlay = root.overlay;
            const origin = overlay.contentItem.mapToGlobal(0, 0);
            const text = root.descendants(overlay.contentItem).find(item => item.objectName === "transcript-text");
            const viewport = root.descendants(overlay.contentItem).find(item => item.objectName === "transcript-viewport");
            const copy = root.descendants(overlay.contentItem).find(item => item.objectName === "copy-button");
            const surface = root.descendants(overlay.contentItem).find(item => item.objectName === "overlay-surface");
            const drag = root.descendants(overlay.contentItem).find(item => item.objectName === "recorder-drag-area");
            const dot = root.descendants(overlay.contentItem).find(item => item.objectName === "recording-dot");
            const status = root.descendants(overlay.contentItem).find(item => item.objectName === "recording-status");
            const header = root.descendants(overlay.contentItem).find(item => item.objectName === "recording-header");
            const timer = root.descendants(overlay.contentItem).find(item => item.objectName === "recording-timer");
            return JSON.stringify({phase: widget.phase, preview: widget.preview, elapsed: widget.elapsed,
                identifier: widget.identifier, options: widget.options, menuOpen: overlay.menuOpen,
                copyEnabled: copy.enabled, copyVisible: copy.visible, renderedText: text.text,
                reviewDuration: overlay.reviewDuration, smoothScrolling: overlay.smoothScrolling,
                scrollDuration: overlay.scrollDuration, scrollLookahead: overlay.scrollLookahead,
                dotOpacity: dot.pulseOpacity, dotScale: dot.pulseScale,
                dotCenterX: dot.x + dot.width / 2, dotCenterY: dot.y + dot.height / 2,
                dotWidth: dot.width, dotHeight: dot.height, dotLabel: dot.Accessible.name, statusText: status.text,
                headerVisible: header.visible, timerVisible: timer.visible, timerText: timer.text,
                headerBottom: header.mapToItem(overlay.contentItem, 0, header.height).y,
                surfaceTop: surface.mapToItem(overlay.contentItem, 0, 0).y,
                viewportTop: viewport.mapToItem(overlay.contentItem, 0, 0).y,
                dotBottom: dot.mapToItem(overlay.contentItem, 0, dot.height).y,
                timerBottom: timer.mapToItem(overlay.contentItem, 0, timer.height).y,
                viewportWidth: viewport.width, textWidth: text.width,
                preset: overlay.positionPreset, headerTransparent: header.color === undefined || header.color.a === 0,
                dragWidth: drag.width, dragHeight: drag.height,
                timerRight: timer.x + timer.width, headerWidth: header.width,
                textHeight: text.height, textY: text.y, viewportHeight: viewport.height, lineHeight: overlay.lineHeight,
                lineCount: text.lineCount, lookAhead: text.lookAhead,
                lastLineFill: text.lastLineFill, previewStart: widget.previewStart,
                leadingIndent: overlay.leadingIndent, discardedHeight: overlay.discardedHeight,
                targetY: Math.min(0, viewport.height - text.height - text.lookAhead),
                surfaceOpacity: surface.color.a, surfaceVisible: surface.visible,
                background: Color.popups.background.toString(), ink: Color.popups.text.toString(),
                visible: overlay.visible, mask: overlay.mask !== null,
                x: origin.x, y: origin.y, width: overlay.width, height: overlay.height, screenWidth: overlay.screen.width,
                screenHeight: overlay.screen.height, focus: input.activeFocus});
    }
    Timer {
        interval: 80
        running: true
        repeat: true
        onTriggered: console.warn("MLUVA_SNAPSHOT " + root.snapshot())
    }
}
