import AppKit
import Observation

public enum AnnotationModifier: String, CaseIterable, Identifiable {
    case shift, control, alt, superKey = "super"

    public var id: String { rawValue }
    public var label: String {
        switch self {
        case .shift: "Shift"
        case .control: "Control"
        case .alt: "Option"
        case .superKey: "Command"
        }
    }
    var flags: NSEvent.ModifierFlags {
        switch self {
        case .shift: .shift
        case .control: .control
        case .alt: .option
        case .superKey: .command
        }
    }
}

/// Screen annotation while dictating: hold the chosen key and move to draw, click to drop a
/// numbered pin. A marked recording becomes a private bundle of keyframes whose summary follows
/// the dictation; screen images are captured only while drawing or pinning.
@MainActor
@Observable
public final class AnnotationController {
    public private(set) var enabled = true
    public private(set) var modifier = AnnotationModifier.shift
    /// Drawing is possible for the current recording.
    public private(set) var active = false
    /// Why annotation is unavailable for this recording, or what went wrong with its bundle.
    public private(set) var notice: String?
    public private(set) var settingsError: String?

    /// Raw frames kept at most, by count and by bytes. They live in memory.
    static let maxFrames = 120
    static let maxFrameBytes = 192 * 1024 * 1024
    /// Frame grabs still running at Stop are waited for this long.
    static let collectTimeout: Duration = .seconds(3)
    /// Strokes smaller than this, in logical pixels, are pointer drift rather than ink.
    static let minStroke = 14.0

    private let core: CoreBridge
    private var session: Session?
    @ObservationIgnored fileprivate var evidenceOverlays: [InkOverlay] = []

    /// One annotated recording, on its own clock.
    private final class Session {
        let owner: String
        let started: Date
        let screens: [AnnotatedScreen]
        let modifier: AnnotationModifier
        var capture = Annotation.Capture()
        var held: [HeldFrame] = []
        var bytes = 0
        var overlays: [InkOverlay] = []
        /// The stroke being drawn: its index, screen and the press origin.
        var ink: (stroke: Int?, screen: AnnotatedScreen, origin: CGPoint)?
        var pins: UInt32 = 0
        var inFlight = 0
        var closed = false
        var ticker: Timer?
        var observers: [NSObjectProtocol] = []
        var preview: () -> Int

        init(owner: String, started: Date, screens: [AnnotatedScreen], modifier: AnnotationModifier, preview: @escaping () -> Int) {
            self.owner = owner
            self.started = started
            self.screens = screens
            self.modifier = modifier
            self.preview = preview
            capture.screens = screens.map(\.model)
        }

        var now: UInt64 { UInt64(max(0, Date().timeIntervalSince(started)) * 1_000) }
    }

    public init(core: CoreBridge = .shared) {
        self.core = core
    }

    /// Read the opt-in once the stores are open.
    public func load() {
        do {
            let settings = try core.call("annotate.settings")
            enabled = settings["enabled"]?.bool ?? true
            modifier = (settings["modifier"]?.string).flatMap { AnnotationModifier(rawValue: $0) } ?? .shift
            settingsError = nil
        } catch {
            enabled = false
            settingsError = "\(error)"
        }
    }

    public func setEnabled(_ value: Bool) {
        do {
            try core.call("annotate.save_settings", ["enabled": .bool(value)])
            enabled = value
            settingsError = nil
            if !value { session.map(stop) }
        } catch {
            settingsError = "\(error)"
        }
    }

    public func setModifier(_ value: AnnotationModifier) {
        do {
            try core.call("annotate.save_settings", ["modifier": .string(value.rawValue)])
            modifier = value
            settingsError = nil
        } catch {
            settingsError = "\(error)"
        }
    }

    public var screenRecordingGranted: Bool { ScreenFrames.permitted }

    // MARK: Recording

    /// A recording began capturing audio. Incognito never annotates.
    public func start(owner: String, startedAt: Date, incognito: Bool, preview: @escaping () -> Int) {
        notice = nil
        guard enabled, session == nil else { return }
        if incognito {
            notice = "Screen annotation is off in Incognito."
            return
        }
        guard ScreenFrames.permitted else {
            notice = "Screen annotation needs Screen Recording access. Recording continues without it."
            return
        }
        let session = Session(owner: owner, started: startedAt, screens: AnnotatedScreen.all(), modifier: modifier, preview: preview)
        self.session = session
        session.overlays = session.screens.map { screen in
            InkOverlay(screen: screen) { [weak self] point in self?.pin(at: point, on: screen) }
        }
        let ticker = Timer(timeInterval: 1.0 / 60, repeats: true) { [weak self] _ in
            MainActor.assumeIsolated { self?.tick() }
        }
        RunLoop.main.add(ticker, forMode: .common)
        session.ticker = ticker
        let center = NSWorkspace.shared.notificationCenter
        session.observers = [
            center.addObserver(forName: NSWorkspace.didActivateApplicationNotification, object: nil, queue: .main) { [weak self] note in
                let app = note.userInfo?[NSWorkspace.applicationUserInfoKey] as? NSRunningApplication
                let name = app?.localizedName ?? app?.bundleIdentifier ?? ""
                let own = app?.processIdentifier == getpid()
                MainActor.assumeIsolated { if !own { self?.focusChanged(.window(t: 0, name: name)) } }
            },
            center.addObserver(forName: NSWorkspace.activeSpaceDidChangeNotification, object: nil, queue: .main) { [weak self] _ in
                MainActor.assumeIsolated { self?.focusChanged(.workspace(t: 0, name: "another Space")) }
            },
        ]
        active = true
    }

    /// End drawing at once: overlays, timers and observers go; grabs in flight still finish.
    public func stop() { session.map(stop) }

    private func stop(_ session: Session) {
        guard !session.closed else { return }
        if let ink = session.ink { endStroke(session, ink) }
        session.closed = true
        session.capture.duration_ms = session.now
        session.ticker?.invalidate()
        session.observers.forEach(NSWorkspace.shared.notificationCenter.removeObserver)
        session.overlays.forEach { $0.close() }
        session.overlays = []
        active = false
    }

    /// Recognition is final: write the bundle and return its summary, or nil when nothing was
    /// marked. The bundle belongs to the recording until `bind` gives it to its conversation.
    public func finish(owner: String, transcript: TranscriptionResult) async -> String? {
        guard let session, session.owner == owner else { return nil }
        stop(session)
        defer { if self.session === session { self.session = nil } }
        guard session.capture.hasMarks else { return nil }
        let deadline = ContinuousClock.now + Self.collectTimeout
        while session.inFlight > 0, ContinuousClock.now < deadline {
            try? await Task.sleep(for: .milliseconds(20))
        }
        let capture = session.capture
        let held = Array(session.held.prefix(capture.frames.count))
        let plan: Annotation.Plan
        do {
            plan = try core.call("annotate.plan", [
                "owner": .string(owner),
                "capture": try JSONValue(encoding: capture),
                "lumas": try JSONValue(encoding: held.map(\.luma)),
                "text": .string(transcript.text),
                "words": try JSONValue(encoding: Annotation.words(transcript)),
            ], as: Annotation.Plan.self)
        } catch {
            notice = "\(error) The dictation was delivered without screen annotation."
            return nil
        }
        do {
            try await Self.draw(plan, capture: capture, held: held)
            let finished = try core.call("annotate.finish", ["identifier": .string(plan.identifier)])
            return finished["summary"]?.string
        } catch {
            _ = try? core.call("annotate.discard", ["identifier": .string(plan.identifier)])
            notice = "Could not draw the screen annotation. The dictation was delivered without it."
            return nil
        }
    }

    /// Give the recording's bundle to its conversation. Without one, it stays with the
    /// recording and is recovered at the next start, since its summary may have been delivered.
    public func bind(owner: String, conversation: String?) {
        guard let conversation else { return }
        if (try? core.call("annotate.bind", ["owner": .string(owner), "conversation": .string(conversation)])) == nil {
            notice = "Could not save screen annotation ownership. It will be recovered at the next start."
        }
    }

    /// Cancel or failure: end the session and drop any bundle the recording owns.
    public func discard(owner: String) {
        if let session, session.owner == owner {
            stop(session)
            self.session = nil
        }
        _ = try? core.call("annotate.discard_owner", ["owner": .string(owner)])
    }

    /// A conversation's finished bundles, newest first.
    public func bundles(for conversation: String) -> [Annotation.Bundle] {
        (try? core.call("annotate.recent", ["id": .string(conversation)], as: [Annotation.Bundle].self)) ?? []
    }

    // MARK: Drawing

    private func pointerScreen(_ session: Session) -> AnnotatedScreen? {
        let pointer = NSEvent.mouseLocation
        return session.screens.first { $0.frame.contains(pointer) } ?? session.screens.first
    }

    private func overlay(_ session: Session, _ screen: AnnotatedScreen) -> InkOverlay? {
        session.overlays.first { $0.screen.name == screen.name }
    }

    /// The selected modifier, read from the system without any permission, drives the ink.
    private func tick() {
        guard let session, !session.closed else { return }
        let flags = NSEvent.modifierFlags.intersection(.deviceIndependentFlagsMask).subtracting([.capsLock, .numericPad, .function])
        let drawing = flags == session.modifier.flags
        session.overlays.forEach { $0.holding = drawing; $0.tick() }
        recordPreview(session)
        guard drawing, let screen = pointerScreen(session) else {
            if let ink = session.ink { endStroke(session, ink) }
            return
        }
        let point = screen.local(NSEvent.mouseLocation)
        guard var ink = session.ink, ink.screen.name == screen.name else {
            if let ink = session.ink { endStroke(session, ink) }
            session.ink = (nil, screen, point)
            return
        }
        let t = session.now
        if let index = ink.stroke {
            let last = session.capture.strokes[index].points.last
            guard let last, hypot(point.x - last.x, point.y - last.y) >= 1.5 else { return }
            session.capture.strokes[index].points.append(.init(x: point.x, y: point.y, t: t))
            session.capture.strokes[index].t1 = t
            overlay(session, screen)?.extend(index, to: point)
        } else if hypot(point.x - ink.origin.x, point.y - ink.origin.y) >= 2 {
            // The stroke begins once the pointer moves, from where the drawing key went down.
            let index = session.capture.strokes.count
            session.capture.strokes.append(.init(
                t0: t, t1: t, screen: screen.name, pin: nil,
                points: [.init(x: ink.origin.x, y: ink.origin.y, t: t), .init(x: point.x, y: point.y, t: t)]))
            ink.stroke = index
            session.ink = ink
            overlay(session, screen)?.begin(index, at: ink.origin)
            overlay(session, screen)?.extend(index, to: point)
            grab(screen)
        }
    }

    private func endStroke(_ session: Session, _ ink: (stroke: Int?, screen: AnnotatedScreen, origin: CGPoint)) {
        session.ink = nil
        guard let index = ink.stroke else { return }
        let points = session.capture.strokes[index].points
        let xs = points.map(\.x), ys = points.map(\.y)
        let size = hypot((xs.max() ?? 0) - (xs.min() ?? 0), (ys.max() ?? 0) - (ys.min() ?? 0))
        if size < Self.minStroke {
            // Drift, not ink: keep the data model free of it.
            session.capture.strokes[index].points = []
            overlay(session, ink.screen)?.release(index, drop: true)
        } else {
            overlay(session, ink.screen)?.release(index)
        }
    }

    private func pin(at point: CGPoint, on screen: AnnotatedScreen) {
        guard let session, !session.closed else { return }
        session.pins += 1
        let t = session.now
        session.capture.strokes.append(.init(t0: t, t1: t, screen: screen.name, pin: session.pins, points: [.init(x: point.x, y: point.y, t: t)]))
        overlay(session, screen)?.pulse(at: point)
        grab(screen)
    }

    private func recordPreview(_ session: Session) {
        let words = UInt64(session.preview())
        guard words > 0, session.capture.preview.last?.last != words else { return }
        session.capture.preview.append([session.now, words])
    }

    private func focusChanged(_ event: Annotation.Event) {
        guard let session, !session.closed, session.capture.events.count < 500 else { return }
        let t = session.now
        switch event {
        case .window(_, let name): session.capture.events.append(.window(t: t, name: name))
        case .workspace(_, let name): session.capture.events.append(.workspace(t: t, name: name))
        }
    }

    /// Capture a marked screen without Mluva's windows.
    private func grab(_ screen: AnnotatedScreen) {
        guard let session, !session.closed else { return }
        let t = session.now
        let windows = ScreenFrames.windows(on: screen)
        session.inFlight += 1
        Task { [weak self] in
            let held: HeldFrame?
            let size: (Int, Int)
            do {
                let image = try await ScreenFrames.capture(screen)
                size = (image.width, image.height)
                held = await Task.detached { ScreenFrames.hold(image) }.value
            } catch {
                held = nil
                size = (0, 0)
            }
            session.inFlight -= 1
            guard let self, let held else { return }
            self.keep(held, in: session, frame: .init(
                t: t, screen: screen.name, reason: .mark, path: "memory:\(t)-\(session.held.count)",
                width: size.0, height: size.1, windows: windows))
        }
    }

    private func keep(_ held: HeldFrame, in session: Session, frame: Annotation.Frame) {
        guard session.capture.frames.count < Self.maxFrames,
              session.bytes + held.data.count <= Self.maxFrameBytes else { return }
        session.capture.frames.append(frame)
        session.held.append(held)
        session.bytes += held.data.count
    }

    /// Render the planned keyframes and contact sheet into the bundle, off the main thread.
    nonisolated static func draw(_ plan: Annotation.Plan, capture: Annotation.Capture, held: [HeldFrame]) async throws {
        try await Task.detached(priority: .userInitiated) {
            var rendered: [CGImage] = []
            for item in plan.keyframes {
                let frame = capture.frames[item.keyframe.frame]
                guard let screen = capture.screens.first(where: { $0.name == frame.screen }),
                      let source = ScreenFrames.image(held[item.keyframe.frame])
                else { throw KeyframeRenderer.Failure.context }
                let image = try KeyframeRenderer.keyframe(
                    source: source, keyframe: item.keyframe, crop: item.crop, marks: plan.marks,
                    strokes: capture.strokes, frame: frame, screen: screen)
                try KeyframeRenderer.writePNG(image, to: URL(fileURLWithPath: item.file))
                rendered.append(image)
            }
            try KeyframeRenderer.writePNG(try KeyframeRenderer.contactSheet(rendered), to: URL(fileURLWithPath: plan.contact_sheet))
        }.value
    }
}

// MARK: Evidence

extension AnnotationController {
    /// Show ink over the screens without recording, for screenshots; nothing is captured.
    @_spi(Evidence)
    public func presentInk(_ strokes: [Annotation.Stroke]) {
        dismissInk()
        let screens = AnnotatedScreen.all()
        evidenceOverlays = screens.map { screen in InkOverlay(screen: screen) { _ in } }
        for (index, stroke) in strokes.enumerated() {
            guard let overlay = evidenceOverlays.first(where: { $0.screen.name == stroke.screen }),
                  let first = stroke.points.first else { continue }
            overlay.begin(index, at: CGPoint(x: first.x, y: first.y))
            for point in stroke.points.dropFirst() { overlay.extend(index, to: CGPoint(x: point.x, y: point.y)) }
        }
    }

    @_spi(Evidence)
    public func dismissInk() {
        evidenceOverlays.forEach { $0.close() }
        evidenceOverlays = []
    }

    /// The name annotation gives the main screen, for synthetic strokes.
    @_spi(Evidence)
    public static func mainScreen() -> (name: String, model: Annotation.Screen)? {
        AnnotatedScreen.all().first.map { ($0.name, $0.model) }
    }

    /// Write a bundle from supplied frames through the same planner, renderer and store.
    @_spi(Evidence)
    public func writeBundle(owner: String, capture: Annotation.Capture, images: [CGImage], text: String) async throws -> String? {
        let held = images.compactMap(ScreenFrames.hold)
        let plan = try core.call("annotate.plan", [
            "owner": .string(owner), "capture": try JSONValue(encoding: capture),
            "lumas": try JSONValue(encoding: held.map(\.luma)), "text": .string(text), "words": [],
        ], as: Annotation.Plan.self)
        try await Self.draw(plan, capture: capture, held: held)
        return try core.call("annotate.finish", ["identifier": .string(plan.identifier)])["summary"]?.string
    }
}
