import AppKit

/// One transparent panel over a screen. It never takes the keyboard, lets every click through
/// until Shift is held, and shows ink that fades shortly after release. Screen capture leaves
/// it out, so keyframes stay clean while the ink is visible.
@MainActor
final class InkOverlay {
    let screen: AnnotatedScreen
    private let panel: NSPanel
    private let canvas: InkCanvas

    init(screen: AnnotatedScreen, onClick: @escaping @MainActor (CGPoint) -> Void) {
        self.screen = screen
        panel = NSPanel(contentRect: screen.frame, styleMask: [.borderless, .nonactivatingPanel], backing: .buffered, defer: false)
        panel.isOpaque = false
        panel.backgroundColor = .clear
        panel.hasShadow = false
        panel.level = .screenSaver
        panel.ignoresMouseEvents = true
        panel.isReleasedWhenClosed = false
        panel.hidesOnDeactivate = false
        panel.collectionBehavior = [.canJoinAllSpaces, .fullScreenAuxiliary, .stationary, .ignoresCycle]
        panel.title = "Mluva screen annotation"
        canvas = InkCanvas(frame: CGRect(origin: .zero, size: screen.frame.size))
        canvas.onClick = onClick
        panel.contentView = canvas
        panel.setFrame(screen.frame, display: false)
        panel.orderFrontRegardless()
    }

    /// While Shift is held the overlay takes clicks, so a Shift-click pins instead of reaching
    /// the application underneath.
    var holding = false {
        didSet {
            guard holding != oldValue else { return }
            panel.ignoresMouseEvents = !holding
            canvas.holding = holding
            canvas.needsDisplay = true
        }
    }

    func begin(_ id: Int, at point: CGPoint) { canvas.lines.append(.init(id: id, points: [point])) }

    func extend(_ id: Int, to point: CGPoint) {
        guard let index = canvas.lines.lastIndex(where: { $0.id == id }) else { return }
        canvas.lines[index].points.append(point)
        canvas.needsDisplay = true
    }

    func release(_ id: Int, drop: Bool = false) {
        guard let index = canvas.lines.lastIndex(where: { $0.id == id }) else { return }
        if drop { canvas.lines.remove(at: index) } else { canvas.lines[index].released = Date() }
        canvas.needsDisplay = true
    }

    func pulse(at point: CGPoint) {
        canvas.pulses.append((point, Date()))
        canvas.needsDisplay = true
    }

    /// Fade and expire ink; called on the controller's tick.
    func tick() {
        if canvas.expire(Date()) { canvas.needsDisplay = true }
    }

    func close() {
        panel.orderOut(nil)
        panel.close()
    }
}

private final class InkCanvas: NSView {
    struct Line {
        let id: Int
        var points: [CGPoint]
        var released: Date?
    }

    /// Ink stays fully visible this long after release, then fades.
    static let hold = 0.7
    static let fade = 0.6
    /// A pin shows as a brief ring; the pin itself stays data.
    static let pulse = 0.45

    var lines: [Line] = []
    var pulses: [(CGPoint, Date)] = []
    var holding = false
    var onClick: (@MainActor (CGPoint) -> Void)?

    override var isFlipped: Bool { true }
    override func acceptsFirstMouse(for event: NSEvent?) -> Bool { true }

    override func mouseDown(with event: NSEvent) {
        onClick?(convert(event.locationInWindow, from: nil))
    }

    /// Whether anything visible changed.
    func expire(_ now: Date) -> Bool {
        let before = (lines.count, pulses.count)
        lines.removeAll { line in line.released.map { now.timeIntervalSince($0) >= Self.hold + Self.fade } ?? false }
        pulses.removeAll { now.timeIntervalSince($0.1) >= Self.pulse }
        let fading = lines.contains { $0.released != nil } || !pulses.isEmpty
        return fading || before != (lines.count, pulses.count)
    }

    override func draw(_ dirtyRect: NSRect) {
        guard let context = NSGraphicsContext.current?.cgContext else { return }
        if holding {
            // A trace of fill, so clicks on otherwise empty pixels land here.
            context.setFillColor(NSColor.black.withAlphaComponent(0.002).cgColor)
            context.fill(bounds)
        }
        let now = Date()
        context.setLineCap(.round)
        context.setLineJoin(.round)
        for line in lines {
            let alpha = line.released.map { released -> Double in
                let age = now.timeIntervalSince(released)
                return age <= Self.hold ? 1 : 1 - min((age - Self.hold) / Self.fade, 1)
            } ?? 1
            guard let first = line.points.first else { continue }
            for (color, width) in [(CGColor(srgbRed: 1, green: 1, blue: 1, alpha: 0.85 * alpha), 7.0),
                                   (CGColor(srgbRed: 1, green: 0.231, blue: 0.188, alpha: alpha), 4.5)] {
                context.beginPath()
                context.move(to: first)
                for point in line.points.dropFirst() { context.addLine(to: point) }
                if line.points.count == 1 { context.addLine(to: CGPoint(x: first.x + 0.1, y: first.y)) }
                context.setStrokeColor(color)
                context.setLineWidth(width)
                context.strokePath()
            }
        }
        for (point, started) in pulses {
            let progress = min(now.timeIntervalSince(started) / Self.pulse, 1)
            let radius = 10 + 18 * progress
            context.setStrokeColor(CGColor(srgbRed: 1, green: 0.231, blue: 0.188, alpha: 1 - progress))
            context.setLineWidth(3)
            context.strokeEllipse(in: CGRect(x: point.x - radius, y: point.y - radius, width: radius * 2, height: radius * 2))
        }
    }
}
