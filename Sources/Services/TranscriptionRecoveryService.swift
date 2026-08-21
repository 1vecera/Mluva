import Foundation

enum TranscriptionRecoveryError: Error, Equatable, LocalizedError {
    case retainedAudioUnavailable
    case emptyTranscript

    var errorDescription: String? {
        switch self {
        case .retainedAudioUnavailable:
            return "The retained audio is no longer available."
        case .emptyTranscript:
            return "The provider returned no speech during retry."
        }
    }
}

final class TranscriptionRecoveryService {
    private let providerFactory: any TranscriptionProviderBuilding
    private let audioRetentionStore: AudioRetentionStore
    private let personalizationStore: PersonalizationStore
    private let transcriptProcessor = TranscriptProcessor()

    init(
        providerFactory: any TranscriptionProviderBuilding = DefaultTranscriptionProviderFactory(),
        audioRetentionStore: AudioRetentionStore = .shared,
        personalizationStore: PersonalizationStore = .shared
    ) {
        self.providerFactory = providerFactory
        self.audioRetentionStore = audioRetentionStore
        self.personalizationStore = personalizationStore
    }

    func retryRecognition(
        entry: TranscriptionEntry,
        settings: AppSettings
    ) async throws -> TranscriptionEntry {
        guard let filename = entry.retainedAudioFilename,
              audioRetentionStore.exists(filename: filename)
        else {
            throw TranscriptionRecoveryError.retainedAudioUnavailable
        }

        let audio = try audioRetentionStore.load(filename: filename)
        let recoverySettings = makeRecoverySettings(for: entry, source: settings)
        let provider = try providerFactory.makeProvider(settings: recoverySettings)
        try await provider.start()

        for offset in stride(from: audio.startIndex, to: audio.endIndex, by: 3_200) {
            let end = audio.index(offset, offsetBy: 3_200, limitedBy: audio.endIndex)
                ?? audio.endIndex
            provider.appendAudio(audio[offset..<end])
        }

        let rawText = try await provider.finish()
            .trimmingCharacters(in: .whitespacesAndNewlines)
        guard !rawText.isEmpty else {
            throw TranscriptionRecoveryError.emptyTranscript
        }
        let processed = transcriptProcessor.process(
            rawText,
            configuration: personalizationStore.processingConfiguration(
                removeFillers: settings.removeFiller,
                targetBundleIdentifier: entry.targetBundleIdentifier
            )
        )

        return TranscriptionEntry(
            id: entry.id,
            title: entry.title,
            rawText: rawText,
            deliveredText: processed.text,
            timestamp: entry.timestamp,
            duration: entry.duration,
            provider: provider.kind,
            language: entry.language,
            mode: entry.mode,
            targetApplicationName: entry.targetApplicationName,
            targetBundleIdentifier: entry.targetBundleIdentifier,
            deliveryOutcome: .pendingDelivery,
            retainedAudioFilename: filename,
            enhancementOutcome: .notRequested
        )
    }

    private func makeRecoverySettings(
        for entry: TranscriptionEntry,
        source: AppSettings
    ) -> AppSettings {
        let defaults = UserDefaults(
            suiteName: "mluva-recovery-\(UUID().uuidString)"
        )!
        let settings = AppSettings(defaults: defaults)
        settings.language = entry.language
        settings.providerPreference = entry.provider == .automatic
            ? source.providerPreference
            : entry.provider
        settings.cloudRecognitionAllowed = source.cloudRecognitionAllowed
        settings.preferCloudForTechnicalSpeech = source.preferCloudForTechnicalSpeech
        settings.requiresOnDeviceAppleSpeech = source.requiresOnDeviceAppleSpeech
        settings.googleCloudProjectID = source.googleCloudProjectID
        settings.googleCloudLocation = source.googleCloudLocation
        settings.googleCloudModel = source.googleCloudModel
        settings.removeFiller = source.removeFiller
        return settings
    }
}
