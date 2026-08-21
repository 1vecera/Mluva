import Testing
@testable import VoiceScribeMac

@Suite("Transcript context")
struct TranscriptContextTests {
    @Test("Context reports only the sources that contain usable content")
    func reportsUsedSources() {
        let context = TranscriptContext(
            applicationName: "Notes",
            windowTitle: "Release plan",
            selectedText: " ",
            nearbyText: "Ship after review"
        )

        #expect(context.sources == [.application, .windowTitle, .nearbyText])
    }

    @Test("Context content is bounded before entering a model request")
    func boundsContextContent() {
        let context = TranscriptContext(
            applicationName: String(repeating: "A", count: 400),
            windowTitle: String(repeating: "W", count: 800),
            selectedText: String(repeating: "S", count: 4_000),
            nearbyText: String(repeating: "N", count: 8_000)
        )

        #expect(context.applicationName?.count == 200)
        #expect(context.windowTitle?.count == 400)
        #expect(context.selectedText?.count == 2_000)
        #expect(context.nearbyText?.count == 4_000)
    }
}
