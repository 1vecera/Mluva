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
    implicitWidth: 16
    implicitHeight: 16
    Accessible.role: Accessible.StaticText
    Accessible.name: active ? "Recording" : "Dictation status"

    // Paint inside a fixed slot: the six-pixel light breathes around its
    // original size without moving the timer, preview or neighboring widgets.
    Rectangle {
        anchors.centerIn: parent
        width: 14
        height: 14
        radius: 7
        color: Qt.alpha(root.ink, 0.12 + 0.04 * root.breath)
        scale: 1 + 0.12 * root.breath
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
        duration: 2600
        loops: Animation.Infinite
        onStopped: root.pulsePhase = 0
    }
}
