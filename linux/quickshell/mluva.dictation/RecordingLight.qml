import QtQuick

Item {
    id: root
    property bool active: false
    property bool animate: true
    property color ink: "#e2554e"
    property real pulsePhase: 0
    readonly property real breath: active && animate ? Math.sin(pulsePhase) : 0
    readonly property real pulseScale: 1 + 0.18 * breath
    readonly property real pulseOpacity: 0.92 + 0.08 * breath
    readonly property real drift: active && animate ? Math.sin(pulsePhase * 2) : 0
    implicitWidth: 20
    implicitHeight: 20
    Accessible.role: Accessible.StaticText
    Accessible.name: active ? "Recording" : "Dictation status"

    // One continuous inhale/exhale, with a slightly yielding outline. All
    // motion stays inside a fixed slot, including the brightest outer ring.
    Rectangle {
        anchors.centerIn: parent
        width: 14
        height: 14
        radius: 7
        color: Qt.alpha(root.ink, 0.10 + 0.04 * root.breath)
        border.width: 0.6
        border.color: Qt.alpha(root.ink, 0.20 + 0.08 * root.breath)
        transform: Scale {
            origin.x: 7; origin.y: 7
            xScale: 1 + 0.14 * root.breath + 0.035 * root.drift
            yScale: 1 + 0.14 * root.breath - 0.035 * root.drift
        }
        visible: root.active
    }
    Rectangle {
        anchors.centerIn: parent
        width: 9; height: 9; radius: 4.5
        color: "transparent"
        border.width: 0.7
        border.color: Qt.alpha(root.ink, 0.38 + 0.12 * root.breath)
        scale: 1 + 0.10 * root.breath
        visible: root.active
    }
    Rectangle {
        anchors.centerIn: parent
        width: 6
        height: 6
        radius: 3
        color: root.ink
        scale: root.pulseScale
        opacity: root.pulseOpacity
    }
    NumberAnimation on pulsePhase {
        running: root.visible && root.active && root.animate
        from: 0
        to: Math.PI * 2
        duration: 3400
        loops: Animation.Infinite
        onStopped: root.pulsePhase = 0
    }
}
