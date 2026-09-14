import QtQuick

Item {
    id: root
    property bool active: false
    property bool animate: true
    property color ink: "#e2554e"
    property real pulsePhase: 0
    readonly property real breath: active && animate ? -Math.cos(pulsePhase) : 0
    implicitWidth: 20
    implicitHeight: 20
    Accessible.role: Accessible.StaticText
    Accessible.name: active ? "Recording" : "Dictation status"

    // One filled, yielding silhouette. No concentric strokes or hard blinking.
    Canvas {
        id: fluid
        anchors.fill: parent
        onPaint: {
            const ctx = getContext("2d");
            ctx.reset();
            const radius = root.active ? 6.4 + 1.6 * root.breath : 3;
            const points = [];
            for (let i = 0; i <= 64; i++) {
                const angle = i / 64 * Math.PI * 2;
                // Deformation and its velocity vanish at both circular endpoints.
                const wobble = root.active && root.animate
                    ? Math.pow(Math.sin(root.pulsePhase), 2)
                        * (0.07 * Math.sin(3 * angle + root.pulsePhase)
                            + 0.035 * Math.sin(5 * angle - 2 * root.pulsePhase)) : 0;
                points.push([(1 + wobble) * Math.cos(angle), (1 + wobble) * Math.sin(angle)]);
            }
            const minX = Math.min(...points.map(point => point[0]));
            const maxX = Math.max(...points.map(point => point[0]));
            const minY = Math.min(...points.map(point => point[1]));
            const maxY = Math.max(...points.map(point => point[1]));
            // Keep the contour's peak bounds exact while its interior shape yields.
            // The recorder places this 20 px slot at x=-2: its 8 px peak meets x=0.
            ctx.beginPath();
            for (let i = 0; i < points.length; i++) {
                const x = width / 2 + radius * (2 * (points[i][0] - minX) / (maxX - minX) - 1);
                const y = height / 2 + radius * (2 * (points[i][1] - minY) / (maxY - minY) - 1);
                if (i === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y);
            }
            ctx.closePath();
            const gradient = ctx.createRadialGradient(width * 0.42, height * 0.4, 0,
                width / 2, height / 2, radius * 1.1);
            gradient.addColorStop(0, Qt.alpha(root.ink, 0.84));
            gradient.addColorStop(0.64, Qt.alpha(root.ink, 0.66));
            gradient.addColorStop(1, Qt.alpha(root.ink, 0.12));
            ctx.fillStyle = gradient;
            ctx.fill();
        }
    }
    onPulsePhaseChanged: fluid.requestPaint()
    onActiveChanged: fluid.requestPaint()
    onInkChanged: fluid.requestPaint()
    onAnimateChanged: fluid.requestPaint()
    NumberAnimation on pulsePhase {
        running: root.visible && root.active && root.animate
        from: 0
        to: Math.PI * 2
        duration: 3400
        loops: Animation.Infinite
        onStopped: root.pulsePhase = 0
    }
}
