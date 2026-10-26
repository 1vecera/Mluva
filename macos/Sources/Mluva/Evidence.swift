import AppKit
import AVFoundation
@_spi(Evidence) import MluvaKit
import SwiftUI

/// `MLUVA_EVIDENCE=<directory>` runs the real app on synthetic data in a private home, opens
/// each surface, saves screenshots and a short recording of its own windows, and quits.
/// It registers no shortcuts, opens no microphone and contacts no provider or Herdr server.
/// macOS lets an app capture its own windows (over the wallpaper) without Screen Recording.
@MainActor
enum Evidence {
    static var directory: URL? {
        ProcessInfo.processInfo.environment["MLUVA_EVIDENCE"].map { URL(fileURLWithPath: $0, isDirectory: true) }
    }

    static func run(_ model: AppModel) {
        guard let directory else { return }
        Task {
            do {
                try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
                try await Script(model: model, directory: directory).run()
            } catch {
                FileHandle.standardError.write(Data("evidence failed: \(error)\n".utf8))
            }
            NSApp.terminate(nil)
        }
    }

    // MARK: Synthetic content

    static let dictations = [
        "Move the export button into the toolbar and keep the shortcut the same.",
        "Reminder for tomorrow: send the draft agenda before ten and book the small meeting room.",
        "The settings page should save on its own; nobody looks for a Save button anymore.",
        "Write a short release note: Mluva now runs natively on the Mac with a menu bar app.",
        "Ask the team whether the onboarding copy reads well in Czech as well as English.",
        "Check why the build cache grows every night and add a cleanup step if it is ours.",
    ]

    static func session(_ name: String, project: String, kind: String = "claude", status: AgentStatus = .idle,
                        guarded: Bool, pane: String = "", pid: UInt32) -> AgentSession {
        let recipient = Recipient(
            socket: guarded || !pane.isEmpty ? "/Users/you/.config/herdr/herdr.sock" : "", server_epoch: guarded ? "e1" : "",
            pane_id: pane, guard: guarded ? "g-\(pid)" : "", name: name, title: name, kind: kind,
            project: "/Users/you/code/\(project)", status: status, session_id: "s-\(pid)", pid: pid)
        let draftOnly = !guarded
        return AgentSession(
            recipient: recipient, display_name: name, draft_only: draftOnly,
            draft_hint: recipient.socket.isEmpty ? "Draft only: it runs outside Herdr" : "Draft only: Herdr can't send to it now",
            can_receive: guarded && status != .blocked, spoken_status: "")
    }

    static let sessions: [AgentSession] = [
        session("Release notes", project: "mluva", guarded: true, pane: "w1:p1", pid: 4101),
        session("Fix flaky upload test", project: "mluva", status: .working, guarded: true, pane: "w1:p2", pid: 4102),
        session("Pricing page copy", project: "site", kind: "codex", guarded: true, pane: "w2:p1", pid: 4103),
        session("Migrate settings schema", project: "core", status: .blocked, guarded: true, pane: "w2:p2", pid: 4104),
        session("Onboarding review", project: "mluva", guarded: false, pid: 4105),
        session("Weekly report", project: "notes", guarded: false, pid: 4106),
        session("Icon experiments", project: "brand", status: .working, guarded: false, pid: 4107),
    ]
}

@MainActor
private struct Script {
    let model: AppModel
    let directory: URL

    func run() async throws {
        try await Task.sleep(for: .seconds(1.5))
        // Windows are captured active, as a person sees them; the previous app returns after.
        let previous = NSWorkspace.shared.frontmostApplication
        NSApp.activate(ignoringOtherApps: true)
        defer { previous?.activate() }
        seedHistory()
        model.evidenceReady = true
        for appearance in [NSAppearance.Name.aqua, .darkAqua] {
            NSApp.appearance = NSAppearance(named: appearance)
            let suffix = appearance == .aqua ? "light" : "dark"
            try await menuPanel(suffix)
            try await pill(suffix)
            try await wheel(suffix)
            try await history(suffix)
        }
        NSApp.appearance = NSAppearance(named: .aqua)
        try await annotation()
        try await settings()
        try await welcome()
        try await recording()
    }

    private func seedHistory() {
        for (index, text) in Evidence.dictations.enumerated() {
            let input = HistoryInput(raw: text.lowercased(), delivered: text, language: "eng",
                                     outcome: index % 3 == 0 ? "pasted" : "copied", application: "com.apple.TextEdit")
            _ = try? CoreBridge.shared.call("history.add", ["input": try JSONValue(encoding: input)])
        }
        model.refreshHistory()
    }

    // MARK: Surfaces

    private func menuPanel(_ suffix: String) async throws {
        guard let button = statusButton() else { return }
        button.performClick(nil)
        try await Task.sleep(for: .seconds(1.2))
        let top = NSScreen.main?.frame.maxY ?? 0
        if let panel = visibleWindows().first(where: { $0.frame.maxY > top - 80 && !($0 is FloatingPanel) }) {
            save(capture(panel, margin: 24), "menu-bar-\(suffix)")
        }
        button.performClick(nil)
        try await Task.sleep(for: .seconds(0.6))
    }

    private func pill(_ suffix: String) async throws {
        let target = Destination.manual
        model.showPill()
        model.dictation.present(.recording, committed: "Move the export button into the toolbar and", volatile: "keep the shortcut",
                                level: 0.45, destination: target, startedAt: Date().addingTimeInterval(-14))
        try await Task.sleep(for: .seconds(1.2))
        if let window = pillWindow() { save(capture(window, margin: 8), "pill-recording-\(suffix)") }
        let session = Evidence.sessions[0]
        let deadline = UInt64(Date().addingTimeInterval(1.3).timeIntervalSince1970 * 1_000)
        model.agents.present(sessions: Evidence.sessions, selected: session.recipient, route: RouteState(
            phase: "grace", id: "a1", reason: nil, deadline_ms: deadline, recipient: session.recipient,
            text: "Summarize what changed since the last release and draft the notes.", history_id: nil))
        model.dictation.present(.finished("Sending"), destination: .agent(session),
                                lastText: "Summarize what changed since the last release and draft the notes.")
        try await Task.sleep(for: .seconds(0.9))
        if let window = pillWindow() { save(capture(window, margin: 8), "pill-agent-countdown-\(suffix)") }
        model.agents.present(sessions: Evidence.sessions, selected: session.recipient, route: nil)
        model.dictation.present(.finished("Pasted into TextEdit."), destination: .manual,
                                lastText: Evidence.dictations[0], notice: "Pasted into TextEdit.")
        try await Task.sleep(for: .seconds(0.9))
        if let window = pillWindow() { save(capture(window, margin: 8), "pill-pasted-\(suffix)") }
        model.dictation.present(.idle)
        try await Task.sleep(for: .seconds(0.6))
    }

    private func wheel(_ suffix: String) async throws {
        model.agents.present(sessions: Evidence.sessions, selected: Evidence.sessions[0].recipient, route: nil)
        model.openWheel()
        model.agents.stopPolling()
        model.agents.present(sessions: Evidence.sessions, selected: Evidence.sessions[0].recipient, route: nil)
        try await Task.sleep(for: .seconds(1.2))
        save(captureScreen(), "wheel-\(suffix)")
        model.agents.present(sessions: Evidence.sessions, selected: Evidence.sessions[4].recipient, route: nil)
        try await Task.sleep(for: .seconds(0.8))
        save(captureScreen(), "wheel-draft-only-\(suffix)")
        model.closeWheel()
        try await Task.sleep(for: .seconds(0.6))
    }

    private func history(_ suffix: String) async throws {
        model.openWindow?("history", model.recent.first?.identifier ?? "")
        try await Task.sleep(for: .seconds(1.5))
        if let window = visibleWindows().first(where: { $0.title == "History" }) {
            window.makeKeyAndOrderFront(nil)
            try await Task.sleep(for: .seconds(0.5))
            save(capture(window, margin: 30), "history-\(suffix)")
            window.close()
        }
        try await Task.sleep(for: .seconds(0.5))
    }

    private func settings() async throws {
        model.settingsTab = .general
        model.openSettingsAction?()
        try await Task.sleep(for: .seconds(1.5))
        if let window = visibleWindows().first(where: { $0.title.contains("General") || $0.identifier?.rawValue.contains("Settings") == true }) {
            window.makeKeyAndOrderFront(nil)
            try await Task.sleep(for: .seconds(0.5))
            save(capture(window, margin: 30), "settings")
            model.settingsTab = .annotation
            try await Task.sleep(for: .seconds(0.8))
            save(capture(window, margin: 30), "settings-annotation")
            window.close()
        }
        model.settingsTab = .general
    }

    /// Ink over a real Mluva window, then the same marks through the shared planner, the
    /// native renderer and the store, shown in History beside their dictation.
    private func annotation() async throws {
        guard let screen = NSScreen.main, let (name, _) = AnnotationController.mainScreen() else { return }
        let said = "Make the Export button red and move it next to Copy. This list should start with today."
        let input = HistoryInput(raw: said.lowercased(), delivered: said, language: "eng", outcome: "pasted", application: "com.apple.TextEdit")
        let entry = try CoreBridge.shared.call("history.add", ["input": try JSONValue(encoding: input)], as: HistoryEntry.self)
        model.refreshHistory()
        model.openWindow?("history", entry.identifier)
        try await Task.sleep(for: .seconds(1.5))
        guard let window = visibleWindows().first(where: { $0.title == "History" }) else { return }
        window.makeKeyAndOrderFront(nil)
        try await Task.sleep(for: .seconds(0.6))
        // The clean view under the ink: Mluva's own windows over the wallpaper.
        guard let clean = Self.createImage?(quartzRect(screen.frame), 1 << 0, 0, 0)?.takeRetainedValue() else { return }

        // Screen-local logical points, origin top left.
        let frame = window.frame
        let top = screen.frame.maxY - frame.maxY
        let left = frame.minX - screen.frame.minX
        func ellipse(cx: Double, cy: Double, rx: Double, ry: Double, t: UInt64) -> [Annotation.Point] {
            (0...36).map { step in
                let angle = Double(step) / 36 * 2.1 * .pi - 0.3
                return .init(x: cx + rx * cos(angle), y: cy + ry * sin(angle), t: t + UInt64(step) * 20)
            }
        }
        let toolbar = ellipse(cx: left + frame.width - 92, cy: top + 26, rx: 92, ry: 26, t: 1_800)
        // Under "This list should start with today." in the dictation's text.
        let underline = (0...20).map { step in
            Annotation.Point(x: left + 642 + Double(step) * 9.6, y: top + 136, t: 4_200 + UInt64(step) * 20)
        }
        let strokes: [Annotation.Stroke] = [
            .init(t0: 1_800, t1: 2_520, screen: name, pin: nil, points: toolbar),
            .init(t0: 4_200, t1: 4_600, screen: name, pin: nil, points: underline),
        ]
        model.annotation.presentInk(strokes)
        try await Task.sleep(for: .seconds(0.6))
        save(captureScreen(screen), "annotation-ink")
        model.annotation.dismissInk()

        guard let (_, screenModel) = AnnotationController.mainScreen() else { return }
        var marked = Annotation.Capture()
        marked.duration_ms = 7_000
        marked.screens = [screenModel]
        marked.strokes = strokes
        let boxes = [Annotation.WindowBox(rect: [left, top, left + frame.width, top + frame.height], active: true)]
        marked.frames = [0, 1_800, 4_200].enumerated().map { index, t in
            Annotation.Frame(t: t, screen: name, reason: index == 0 ? .start : .mark, path: "memory:\(index)",
                             width: clean.width, height: clean.height, windows: boxes)
        }
        let owner = "evidence-\(entry.identifier)"
        if let summary = try await model.annotation.writeBundle(owner: owner, capture: marked, images: [clean, clean, clean], text: said) {
            try summary.write(to: directory.appending(path: "annotation-summary.md"), atomically: true, encoding: .utf8)
        }
        model.annotation.bind(owner: owner, conversation: entry.identifier)
        if let bundle = model.annotation.bundles(for: entry.identifier).first {
            for (index, path) in bundle.keyframes.enumerated() {
                let target = directory.appending(path: "annotation-keyframe-\(index + 1).png")
                try? FileManager.default.removeItem(at: target)
                try? FileManager.default.copyItem(at: URL(fileURLWithPath: path), to: target)
            }
        }
        window.close()
        try await Task.sleep(for: .seconds(0.5))
        model.openWindow?("history", entry.identifier)
        try await Task.sleep(for: .seconds(1.5))
        if let window = visibleWindows().first(where: { $0.title == "History" }) {
            window.makeKeyAndOrderFront(nil)
            try await Task.sleep(for: .seconds(0.8))
            save(capture(window, margin: 30), "history-annotation")
            window.close()
        }
        try await Task.sleep(for: .seconds(0.5))
    }

    private func welcome() async throws {
        model.evidenceReady = false
        model.openWindow?("welcome", nil)
        try await Task.sleep(for: .seconds(1.8))
        if let window = visibleWindows().first(where: { $0.title.contains("Welcome") }) {
            window.makeKeyAndOrderFront(nil)
            try await Task.sleep(for: .seconds(0.5))
            save(capture(window, margin: 30), "welcome")
            window.close()
        }
        model.evidenceReady = true
    }

    // MARK: Recording

    /// A short screen recording of the pill and wheel on this Mac's own screen.
    private func recording() async throws {
        guard let screen = NSScreen.main else { return }
        let url = directory.appending(path: "mluva-macos.mp4")
        try? FileManager.default.removeItem(at: url)
        let recorder = try Recorder(url: url, size: CGSize(width: 1280, height: 720))
        let words = "Mluva now runs natively on the Mac. Press F9 anywhere, talk, and press it again. The words land in the field you were typing in.".split(separator: " ")
        let frames = Task { @MainActor in
            while !Task.isCancelled {
                if let image = captureScreen(screen) { recorder.append(image) }
                try? await Task.sleep(for: .milliseconds(33))
            }
        }
        model.showPill()
        model.dictation.present(.preparing, destination: .target(nil))
        try await Task.sleep(for: .seconds(0.8))
        let started = Date()
        for count in 1...words.count {
            let committed = words.prefix(max(0, count - 3)).joined(separator: " ")
            let volatile = words.dropFirst(max(0, count - 3)).prefix(3).joined(separator: " ")
            model.dictation.present(.recording, committed: committed, volatile: volatile,
                                    level: Float.random(in: 0.2...0.8), destination: .target(nil), startedAt: started)
            try await Task.sleep(for: .milliseconds(230))
        }
        model.dictation.present(.finalizing, committed: words.joined(separator: " "), destination: .target(nil))
        try await Task.sleep(for: .seconds(0.9))
        model.dictation.present(.finished("Pasted into TextEdit."), destination: .target(nil),
                                lastText: words.joined(separator: " "), notice: "Pasted into TextEdit.")
        try await Task.sleep(for: .seconds(1.6))
        model.dictation.present(.idle)
        try await Task.sleep(for: .seconds(0.6))
        model.agents.present(sessions: Evidence.sessions, selected: Evidence.sessions[0].recipient, route: nil)
        model.openWheel()
        model.agents.stopPolling()
        for index in [0, 2, 4, 1] {
            model.agents.present(sessions: Evidence.sessions, selected: Evidence.sessions[index].recipient, route: nil)
            try await Task.sleep(for: .seconds(0.9))
        }
        model.closeWheel()
        try await Task.sleep(for: .seconds(0.8))
        frames.cancel()
        await recorder.finish()
    }

    // MARK: Capture

    private func statusButton() -> NSStatusBarButton? {
        func find(_ view: NSView?) -> NSStatusBarButton? {
            guard let view else { return nil }
            if let button = view as? NSStatusBarButton { return button }
            return view.subviews.lazy.compactMap(find).first
        }
        return NSApp.windows.lazy.compactMap { find($0.contentView) }.first
    }

    private func visibleWindows() -> [NSWindow] {
        NSApp.windows.filter { $0.isVisible && !String(describing: type(of: $0)).contains("StatusBar") }
    }

    private func pillWindow() -> NSWindow? {
        visibleWindows().first { ($0 as? FloatingPanel)?.canBecomeKey == false }
    }

    private typealias CreateImage = @convention(c) (CGRect, UInt32, UInt32, UInt32) -> Unmanaged<CGImage>?

    /// `CGWindowListCreateImage`, looked up at run time: the SDK marks it obsolete, but it still
    /// returns this app's own windows and the wallpaper without Screen Recording permission.
    private static let createImage: CreateImage? = {
        guard let symbol = dlsym(dlopen(nil, RTLD_NOW), "CGWindowListCreateImage") else { return nil }
        return unsafeBitCast(symbol, to: CreateImage.self)
    }()

    private func quartzRect(_ frame: NSRect) -> CGRect {
        let height = NSScreen.screens.first?.frame.height ?? 0
        return CGRect(x: frame.minX, y: height - frame.maxY, width: frame.width, height: frame.height)
    }

    private func capture(_ window: NSWindow, margin: CGFloat) -> CGImage? {
        let rect = quartzRect(window.frame).insetBy(dx: -margin, dy: -margin)
        // On-screen windows below and including this one: its glass over what it floats on.
        return Self.createImage?(rect, 1 << 2 | 1 << 3, UInt32(window.windowNumber), 0)?.takeRetainedValue()
    }

    /// The screen below the menu bar, which belongs to whichever app is frontmost.
    private func captureScreen(_ screen: NSScreen? = NSScreen.main) -> CGImage? {
        guard let screen else { return nil }
        var frame = screen.frame
        frame.size.height = screen.visibleFrame.maxY - frame.minY
        return Self.createImage?(quartzRect(frame), 1 << 0, 0, 0)?.takeRetainedValue()
    }

    private func save(_ image: CGImage?, _ name: String) {
        guard let image else { return }
        let url = directory.appending(path: "\(name).png")
        guard let destination = CGImageDestinationCreateWithURL(url as CFURL, "public.png" as CFString, 1, nil) else { return }
        CGImageDestinationAddImage(destination, image, nil)
        CGImageDestinationFinalize(destination)
    }
}

/// H.264 frames from captured images.
@MainActor
private final class Recorder {
    private let writer: AVAssetWriter
    private let input: AVAssetWriterInput
    private let adaptor: AVAssetWriterInputPixelBufferAdaptor
    private let size: CGSize
    private let start = Date()

    init(url: URL, size: CGSize) throws {
        self.size = size
        writer = try AVAssetWriter(outputURL: url, fileType: .mp4)
        input = AVAssetWriterInput(mediaType: .video, outputSettings: [
            AVVideoCodecKey: AVVideoCodecType.h264, AVVideoWidthKey: size.width, AVVideoHeightKey: size.height,
        ])
        input.expectsMediaDataInRealTime = true
        adaptor = AVAssetWriterInputPixelBufferAdaptor(assetWriterInput: input, sourcePixelBufferAttributes: [
            kCVPixelBufferPixelFormatTypeKey as String: kCVPixelFormatType_32BGRA,
            kCVPixelBufferWidthKey as String: size.width, kCVPixelBufferHeightKey as String: size.height,
        ])
        writer.add(input)
        writer.startWriting()
        writer.startSession(atSourceTime: .zero)
    }

    func append(_ image: CGImage) {
        guard input.isReadyForMoreMediaData, let pool = adaptor.pixelBufferPool else { return }
        var buffer: CVPixelBuffer?
        CVPixelBufferPoolCreatePixelBuffer(nil, pool, &buffer)
        guard let buffer else { return }
        CVPixelBufferLockBaseAddress(buffer, [])
        if let context = CGContext(
            data: CVPixelBufferGetBaseAddress(buffer), width: Int(size.width), height: Int(size.height),
            bitsPerComponent: 8, bytesPerRow: CVPixelBufferGetBytesPerRow(buffer),
            space: CGColorSpaceCreateDeviceRGB(),
            bitmapInfo: CGImageAlphaInfo.premultipliedFirst.rawValue | CGBitmapInfo.byteOrder32Little.rawValue) {
            context.interpolationQuality = .high
            context.draw(image, in: CGRect(origin: .zero, size: size))
        }
        CVPixelBufferUnlockBaseAddress(buffer, [])
        let time = CMTime(seconds: Date().timeIntervalSince(start), preferredTimescale: 600)
        adaptor.append(buffer, withPresentationTime: time)
    }

    func finish() async {
        input.markAsFinished()
        await writer.finishWriting()
    }
}
