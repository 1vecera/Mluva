import CoreGraphics
import Foundation
import ImageIO
import Testing
@testable import MluvaKit

/// A synthetic screen, drawn here: grey with a dark "button" where the ink circles.
private func syntheticScreen(width: Int, height: Int, shade: Double) throws -> CGImage {
    let context = try KeyframeRenderer.canvas(width: width, height: height)
    context.setFillColor(CGColor(gray: shade, alpha: 1))
    context.fill(CGRect(x: 0, y: 0, width: width, height: height))
    context.setFillColor(CGColor(gray: 0.2, alpha: 1))
    context.fill(CGRect(x: 760, y: 560, width: 480, height: 160))
    return try #require(context.makeImage())
}

private func pixel(_ image: CGImage, x: Int, y: Int) throws -> (Int, Int, Int) {
    var data = [UInt8](repeating: 0, count: 4)
    let context = try #require(CGContext(
        data: &data, width: 1, height: 1, bitsPerComponent: 8, bytesPerRow: 4,
        space: CGColorSpace(name: CGColorSpace.sRGB)!, bitmapInfo: CGImageAlphaInfo.noneSkipLast.rawValue))
    // CoreGraphics draws bottom up; shift so the wanted top-left pixel lands at the origin.
    context.draw(image, in: CGRect(x: -x, y: -(image.height - 1 - y), width: image.width, height: image.height))
    return (Int(data[0]), Int(data[1]), Int(data[2]))
}

extension CoreBridgeTests {
    @MainActor @Test func aMarkedRecordingIsPlannedSharedDrawnNativelyAndOwnedByItsConversation() async throws {
        let controller = AnnotationController()
        controller.load()
        #expect(controller.enabled)

        // A 1440×900 point screen captured at 2×, with a circle drawn around the button.
        let screen = Annotation.Screen(name: "Built-in Display", x: 0, y: 0, width: 1440, height: 900, scale: 2)
        let images = [try syntheticScreen(width: 2880, height: 1800, shade: 0.9), try syntheticScreen(width: 2880, height: 1800, shade: 0.85)]
        let held = images.map { try! #require(ScreenFrames.hold($0)) }
        var circle: [Annotation.Point] = []
        for step in 0...24 {
            let angle = Double(step) / 24 * 2 * .pi
            circle.append(.init(x: 500 + 160 * cos(angle), y: 320 + 70 * sin(angle), t: 2_000 + UInt64(step) * 25))
        }
        var capture = Annotation.Capture()
        capture.duration_ms = 6_000
        capture.screens = [screen]
        capture.strokes = [.init(t0: 2_000, t1: 2_600, screen: screen.name, pin: nil, points: circle),
                           .init(t0: 4_000, t1: 4_000, screen: screen.name, pin: 1, points: [.init(x: 1_100, y: 600, t: 4_000)])]
        capture.frames = [
            .init(t: 0, screen: screen.name, reason: .start, path: "memory:0", width: 2880, height: 1800, windows: []),
            .init(t: 2_000, screen: screen.name, reason: .mark, path: "memory:1", width: 2880, height: 1800,
                  windows: [.init(rect: [200, 100, 1300, 820], active: true)]),
        ]
        capture.events = [.window(t: 1_000, name: "Notes")]

        let plan = try CoreBridge.shared.call("annotate.plan", [
            "owner": "capture-swift",
            "capture": try JSONValue(encoding: capture),
            "lumas": try JSONValue(encoding: held.map(\.luma)),
            "text": "Make this button red and move it here.",
            "words": [],
        ], as: Annotation.Plan.self)
        #expect(!plan.keyframes.isEmpty)
        #expect(plan.marks.contains { $0.shape == "pin" })

        try await AnnotationController.draw(plan, capture: capture, held: held)
        let finished = try CoreBridge.shared.call("annotate.finish", ["identifier": .string(plan.identifier)])
        let summary = try #require(finished["summary"]?.string)
        #expect(summary.contains("01.png"))

        // The badge strip sits above the frame and the ink is drawn in the shared red.
        let first = try #require(plan.keyframes.first)
        let source = try #require(CGImageSourceCreateWithURL(URL(fileURLWithPath: first.file) as CFURL, nil))
        let rendered = try #require(CGImageSourceCreateImageAtIndex(source, 0, nil))
        #expect(max(rendered.width, rendered.height) <= 1_600 + 44)
        let attributes = try FileManager.default.attributesOfItem(atPath: first.file)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
        let badge = try pixel(rendered, x: 12, y: 22)
        #expect(badge.0 > 200 && badge.1 < 120, "badge \(badge)")

        let entry = try CoreBridge.shared.call("history.add", [
            "input": try JSONValue(encoding: HistoryInput(raw: "Make this button red", delivered: "Make this button red", language: "eng", outcome: "copied")),
        ], as: HistoryEntry.self)
        controller.bind(owner: "capture-swift", conversation: entry.identifier)
        let bundles = controller.bundles(for: entry.identifier)
        #expect(bundles.map(\.identifier) == [plan.identifier])
        #expect(bundles.first?.keyframes.count == plan.keyframes.count)
    }

    @MainActor @Test func aRecordingWithoutMarksPlansNothingAndDiscardingLeavesNoBundle() throws {
        var capture = Annotation.Capture()
        capture.duration_ms = 3_000
        capture.screens = [.init(name: "Built-in Display", x: 0, y: 0, width: 1440, height: 900, scale: 2)]
        #expect(throws: (any Error).self) {
            try CoreBridge.shared.call("annotate.plan", [
                "owner": "capture-empty", "capture": try JSONValue(encoding: capture), "text": "Nothing marked.",
            ])
        }
        let controller = AnnotationController()
        controller.discard(owner: "capture-empty")
        #expect(controller.notice == nil)
    }

    @MainActor @Test func screenDifferenceUsesTheSharedPlannersMeasure() throws {
        let same = [Float](repeating: 0.5, count: 64 * 36)
        var banner = same
        for i in 0..<(64 * 6) { banner[i] = 0.95 }
        let none = try #require(CoreBridge.shared.call("annotate.diff", ["a": try JSONValue(encoding: same), "b": try JSONValue(encoding: same)]).number)
        let changed = try #require(CoreBridge.shared.call("annotate.diff", ["a": try JSONValue(encoding: same), "b": try JSONValue(encoding: banner)]).number)
        #expect(none == 0)
        #expect(changed > Double(AnnotationController.sampleNovelty))
    }
}
