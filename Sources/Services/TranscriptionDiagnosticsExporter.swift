import Foundation

struct TranscriptionDiagnosticsExporter {
    private struct Report: Codable {
        let schemaVersion: Int
        let generatedAt: Date
        let configuration: Configuration
        let sessions: [Session]
    }

    private struct Configuration: Codable {
        let language: String
        let providerPreference: TranscriptionProviderKind
        let cloudRecognitionAllowed: Bool
        let googleCloudConfigured: Bool
        let googleCloudLocation: String
        let googleCloudModel: String
        let requiresOnDeviceAppleSpeech: Bool
    }

    private struct Session: Codable {
        let timestamp: Date
        let captureDuration: TimeInterval
        let provider: TranscriptionProviderKind
        let language: String
        let mode: TranscriptionMode
        let deliveryOutcome: TranscriptionDeliveryOutcome
        let failureCategory: FailureCategory?
        let fallbackEvent: ProviderFallbackEvent?
        let timings: TranscriptionTimings
    }

    private enum FailureCategory: String, Codable {
        case recognitionOrProcessing
        case delivery
    }

    private let now: () -> Date

    init(now: @escaping () -> Date = Date.init) {
        self.now = now
    }

    func export(
        settings: AppSettings,
        entries: [TranscriptionEntry]
    ) throws -> Data {
        let report = Report(
            schemaVersion: 1,
            generatedAt: now(),
            configuration: Configuration(
                language: settings.language,
                providerPreference: settings.providerPreference,
                cloudRecognitionAllowed: settings.cloudRecognitionAllowed,
                googleCloudConfigured: !settings.googleCloudProjectID.isEmpty,
                googleCloudLocation: settings.googleCloudLocation,
                googleCloudModel: settings.googleCloudModel,
                requiresOnDeviceAppleSpeech: settings.requiresOnDeviceAppleSpeech
            ),
            sessions: entries.map { entry in
                Session(
                    timestamp: entry.timestamp,
                    captureDuration: entry.duration,
                    provider: entry.provider,
                    language: entry.language,
                    mode: entry.mode,
                    deliveryOutcome: entry.deliveryOutcome,
                    failureCategory: failureCategory(for: entry),
                    fallbackEvent: entry.fallbackEvent,
                    timings: entry.timings
                )
            }
        )
        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
        return try encoder.encode(report)
    }

    private func failureCategory(for entry: TranscriptionEntry) -> FailureCategory? {
        switch entry.deliveryOutcome {
        case .delivered:
            nil
        case .pendingDelivery:
            entry.failureMessage == nil ? nil : .delivery
        case .failed:
            .recognitionOrProcessing
        }
    }
}
