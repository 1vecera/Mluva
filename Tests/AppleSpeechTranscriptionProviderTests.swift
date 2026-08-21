import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Apple Speech transcription provider")
struct AppleSpeechTranscriptionProviderTests {
    @Test("Apple partial and final recognition share one segment identity")
    func partialAndFinalShareIdentity() async throws {
        let engine = RecordingAppleSpeechEngine()
        let provider = AppleSpeechTranscriptionProvider(
            configuration: AppleSpeechConfiguration(
                localeIdentifier: "en-US",
                requiresOnDeviceRecognition: true,
                contextualStrings: ["Mluva", "Postgres"]
            ),
            engine: engine,
            makeEventID: { "apple-segment" }
        )
        var events: [TranscriptEvent] = []
        provider.onEvent = { events.append($0) }

        try await provider.start()
        provider.appendAudio(Data([0x01, 0x02]))
        engine.emit(text: "hello wor", isFinal: false)
        engine.emit(text: "hello world", isFinal: true)
        let transcript = try await provider.finish()

        #expect(engine.startedLocale == "en-US")
        #expect(engine.requiredOnDeviceRecognition)
        #expect(engine.contextualStrings == ["Mluva", "Postgres"])
        #expect(engine.audio == Data([0x01, 0x02]))
        #expect(events == [
            .volatile(id: "apple-segment", text: "hello wor"),
            .final(id: "apple-segment", text: "hello world"),
        ])
        #expect(transcript == "hello world")
    }

    @Test("Apple finish uses the latest recognition when no final callback arrives")
    func finishUsesLatestRecognition() async throws {
        let engine = RecordingAppleSpeechEngine()
        let provider = AppleSpeechTranscriptionProvider(
            configuration: AppleSpeechConfiguration(localeIdentifier: "cs-CZ"),
            engine: engine,
            makeEventID: { "apple-segment" }
        )

        try await provider.start()
        engine.emit(text: "ahoj světe", isFinal: false)
        let transcript = try await provider.finish()

        #expect(transcript == "ahoj světe")
    }

    @Test("Legacy Apple sessions roll with audio overlap and transcript deduplication")
    func legacySessionRollover() async throws {
        let first = FinishingAppleSpeechEngine(finalText: "hello world")
        let second = FinishingAppleSpeechEngine(finalText: "world again")
        var engines = [first, second]
        let engine = RollingAppleSpeechEngine(
            rolloverAudioBytes: 4,
            overlapAudioBytes: 2,
            makeEngine: { engines.removeFirst() }
        )
        var recognitions: [(String, Bool)] = []
        engine.onRecognition = { recognitions.append(($0, $1)) }

        try await engine.start(
            localeIdentifier: "en-US",
            requiresOnDeviceRecognition: true,
            contextualStrings: ["Mluva"]
        )
        try engine.appendPCM16(Data([0, 1, 2, 3, 4, 5]))
        try await engine.finish()

        #expect(first.audio == Data([0, 1, 2, 3]))
        #expect(second.audio == Data([2, 3, 4, 5]))
        #expect(first.startCount == 1)
        #expect(second.startCount == 1)
        #expect(first.contextualStrings == ["Mluva"])
        #expect(second.contextualStrings == ["Mluva"])
        #expect(recognitions.filter(\.1).map(\.0) == ["hello world again"])
    }
}

private final class RecordingAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?
    private(set) var startedLocale: String?
    private(set) var requiredOnDeviceRecognition = false
    private(set) var contextualStrings: [String] = []
    private(set) var audio = Data()

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        startedLocale = localeIdentifier
        requiredOnDeviceRecognition = requiresOnDeviceRecognition
        self.contextualStrings = contextualStrings
    }

    func appendPCM16(_ data: Data) throws {
        audio.append(data)
    }

    func finish() async throws {}
    func cancel() {}

    func emit(text: String, isFinal: Bool) {
        onRecognition?(text, isFinal)
    }
}

private final class FinishingAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?
    private let finalText: String
    private(set) var audio = Data()
    private(set) var startCount = 0
    private(set) var contextualStrings: [String] = []

    init(finalText: String) {
        self.finalText = finalText
    }

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        startCount += 1
        self.contextualStrings = contextualStrings
    }

    func appendPCM16(_ data: Data) throws {
        audio.append(data)
    }

    func finish() async throws {
        onRecognition?(finalText, true)
    }

    func cancel() {}
}
