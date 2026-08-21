import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Transcription recovery service")
struct TranscriptionRecoveryServiceTests {
    @Test("Retry recognizes retained audio without recording again")
    func retriesRetainedAudio() async throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("recovery-\(UUID().uuidString)")
        let audioStore = AudioRetentionStore(directoryURL: directory)
        let provider = RecoveryTranscriptionProvider(transcript: "use post grass")
        let entryID = UUID()
        let audio = Data([0x01, 0x02, 0x03, 0x04])
        let filename = try audioStore.save(audio, for: entryID)
        let entry = TranscriptionEntry(
            id: entryID,
            rawText: "",
            deliveredText: "",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .failed,
            retainedAudioFilename: filename
        )
        let personalization = PersonalizationStore(
            fileURL: directory.appendingPathComponent("personalization.json")
        )
        personalization.saveDictionaryReplacement(spoken: "post grass", written: "Postgres")
        let service = TranscriptionRecoveryService(
            providerFactory: RecoveryProviderFactory(provider: provider),
            audioRetentionStore: audioStore,
            personalizationStore: personalization
        )
        let settings = AppSettings(
            defaults: UserDefaults(suiteName: "recovery-settings-\(UUID().uuidString)")!
        )

        let recovered = try await service.retryRecognition(entry: entry, settings: settings)

        #expect(provider.audio == audio)
        #expect(recovered.id == entryID)
        #expect(recovered.rawText == "use post grass")
        #expect(recovered.deliveredText == "use Postgres")
        #expect(recovered.deliveryOutcome == .pendingDelivery)
        #expect(recovered.retainedAudioFilename == filename)
    }

    @Test("Retry reports missing retained audio")
    func reportsMissingAudio() async {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("missing-recovery-\(UUID().uuidString)")
        let service = TranscriptionRecoveryService(
            providerFactory: RecoveryProviderFactory(
                provider: RecoveryTranscriptionProvider(transcript: "unused")
            ),
            audioRetentionStore: AudioRetentionStore(directoryURL: directory),
            personalizationStore: PersonalizationStore(
                fileURL: directory.appendingPathComponent("personalization.json")
            )
        )
        let entry = TranscriptionEntry(
            rawText: "",
            deliveredText: "",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .failed,
            retainedAudioFilename: "missing.pcm"
        )

        await #expect(throws: TranscriptionRecoveryError.retainedAudioUnavailable) {
            try await service.retryRecognition(entry: entry, settings: .shared)
        }
    }
}

private struct RecoveryProviderFactory: TranscriptionProviderBuilding {
    let provider: RecoveryTranscriptionProvider

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        provider
    }
}

private final class RecoveryTranscriptionProvider: TranscriptionProvider {
    let kind: TranscriptionProviderKind = .apple
    var onEvent: ((TranscriptEvent) -> Void)?
    private(set) var audio = Data()
    private let transcript: String

    init(transcript: String) {
        self.transcript = transcript
    }

    func start() async throws {}
    func appendAudio(_ data: Data) { audio.append(data) }
    func finish() async throws -> String { transcript }
    func cancel() {}
}
