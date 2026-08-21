import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Meeting controller")
struct MeetingControllerTests {
    @Test("Explicit meeting capture archives mixed audio without dictation delivery")
    func archivesMeeting() async throws {
        let defaults = UserDefaults(suiteName: "meeting-controller-\(UUID().uuidString)")!
        let settings = AppSettings(defaults: defaults)
        settings.providerPreference = .apple
        settings.language = "en"
        let provider = MeetingTestProvider(
            transcript: "We decided to ship Friday. Daniel will update the notes."
        )
        let capture = MeetingTestAudioCapture()
        let writer = MeetingTestRecordingWriter()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("meeting-controller-\(UUID().uuidString)")
        let fileURL = directory.appendingPathComponent("meetings.json")
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = MeetingStore(fileURL: fileURL)
        let controller = MeetingController(
            settings: settings,
            providerFactory: MeetingTestProviderFactory(provider: provider),
            audioCapture: capture,
            meetingStore: store,
            recordingWriter: writer,
            now: { Date(timeIntervalSince1970: 1_800_000_000) }
        )

        try await controller.start()
        capture.emit(Data([0x01, 0x00, 0x02, 0x00]))
        let meeting = try await controller.stop()

        #expect(provider.audio == Data([0x01, 0x00, 0x02, 0x00]))
        #expect(writer.audio == provider.audio)
        #expect(meeting?.audioSources == [.microphone, .system])
        #expect(meeting?.insights.decisions == ["We decided to ship Friday."])
        #expect(meeting?.recordingFilename == "meeting.wav")
        #expect(meeting?.id == writer.startedID)
        #expect(store.meetings == (meeting.map { [$0] } ?? []))
        #expect(controller.state == .idle)
    }

    @Test("Incognito meeting keeps no archive or recording")
    func incognitoMeeting() async throws {
        let defaults = UserDefaults(suiteName: "meeting-incognito-\(UUID().uuidString)")!
        let settings = AppSettings(defaults: defaults)
        settings.providerPreference = .apple
        settings.incognitoMode = true
        let provider = MeetingTestProvider(transcript: "Private meeting notes.")
        let capture = MeetingTestAudioCapture()
        let writer = MeetingTestRecordingWriter()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("meeting-incognito-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let store = MeetingStore(fileURL: directory.appendingPathComponent("meetings.json"))
        let controller = MeetingController(
            settings: settings,
            providerFactory: MeetingTestProviderFactory(provider: provider),
            audioCapture: capture,
            meetingStore: store,
            recordingWriter: writer
        )

        try await controller.start()
        capture.emit(Data([0x01, 0x00]))
        settings.incognitoMode = false
        let meeting = try await controller.stop()

        #expect(meeting?.recordingFilename == nil)
        #expect(store.meetings.isEmpty)
        #expect(writer.wasDiscarded)
        #expect(controller.state == .idle)
    }
}

private final class MeetingTestProvider: TranscriptionProvider {
    let kind: TranscriptionProviderKind = .apple
    var onEvent: ((TranscriptEvent) -> Void)?
    private let transcript: String
    private(set) var audio = Data()

    init(transcript: String) {
        self.transcript = transcript
    }

    func start() async throws {}
    func appendAudio(_ data: Data) { audio.append(data) }
    func finish() async throws -> String { transcript }
    func cancel() {}
}

private struct MeetingTestProviderFactory: TranscriptionProviderBuilding {
    let provider: any TranscriptionProvider

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        provider
    }
}

private final class MeetingTestAudioCapture: MeetingAudioCapturing {
    var onAudioChunk: ((Data) -> Void)?
    var onError: ((any Error) -> Void)?

    func start() async throws {}
    func stop() async {}
    func emit(_ data: Data) { onAudioChunk?(data) }
}

private final class MeetingTestRecordingWriter: MeetingRecordingWriting {
    private(set) var audio = Data()
    private(set) var startedID: UUID?
    private(set) var wasDiscarded = false

    func start(id: UUID) throws -> String {
        startedID = id
        return "meeting.wav"
    }
    func append(_ data: Data) throws { audio.append(data) }
    func finish() throws {}
    func discard() { wasDiscarded = true }
}
