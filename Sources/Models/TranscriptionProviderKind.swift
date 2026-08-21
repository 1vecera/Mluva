import Foundation

enum TranscriptionProviderKind: String, Codable, CaseIterable, Sendable {
    case automatic
    case apple
    case googleCloud

    var displayName: String {
        switch self {
        case .automatic: "Automatic"
        case .apple: "Apple Speech"
        case .googleCloud: "Google Cloud"
        }
    }
}

enum RecognitionPermissionRequirement: Equatable, Sendable {
    case microphone
    case appleSpeech
}

struct RecognitionPermissionPolicy {
    static func missingRequirement(
        preference: TranscriptionProviderKind,
        microphoneGranted: Bool,
        appleSpeechGranted: Bool,
        cloudFallbackAvailable: Bool
    ) -> RecognitionPermissionRequirement? {
        guard microphoneGranted else { return .microphone }

        switch preference {
        case .apple:
            return appleSpeechGranted ? nil : .appleSpeech
        case .googleCloud:
            return nil
        case .automatic:
            return appleSpeechGranted || cloudFallbackAvailable ? nil : .appleSpeech
        }
    }
}

struct ProviderRoutingRequest: Equatable, Sendable {
    let preference: TranscriptionProviderKind
    let language: String
    let needsCloudAccuracy: Bool
}

struct ProviderCapabilities: Equatable, Sendable {
    let appleOnDeviceLanguages: Set<String>
    let appleRecognitionAuthorized: Bool
    let googleCloudConfigured: Bool
    let cloudAllowed: Bool

    init(
        appleOnDeviceLanguages: Set<String>,
        appleRecognitionAuthorized: Bool = true,
        googleCloudConfigured: Bool,
        cloudAllowed: Bool
    ) {
        self.appleOnDeviceLanguages = appleOnDeviceLanguages
        self.appleRecognitionAuthorized = appleRecognitionAuthorized
        self.googleCloudConfigured = googleCloudConfigured
        self.cloudAllowed = cloudAllowed
    }

    func appleSupports(_ language: String) -> Bool {
        language == "auto" || appleOnDeviceLanguages.contains {
            $0.caseInsensitiveCompare(language) == .orderedSame
        }
    }
}

enum ProviderRoutingError: Error, Equatable, LocalizedError {
    case appleLanguageUnavailable(String)
    case applePermissionRequired
    case googleCloudNotConfigured
    case cloudNotPermitted
    case noPermittedProvider

    var errorDescription: String? {
        switch self {
        case .appleLanguageUnavailable(let language):
            return "Apple on-device recognition is unavailable for \(language)."
        case .applePermissionRequired:
            return "Apple Speech permission is required for Apple recognition."
        case .googleCloudNotConfigured:
            return "Google Cloud Speech-to-Text is not configured."
        case .cloudNotPermitted:
            return "Cloud recognition is disabled by the current privacy policy."
        case .noPermittedProvider:
            return "No transcription provider can handle this request under the current privacy policy."
        }
    }
}
