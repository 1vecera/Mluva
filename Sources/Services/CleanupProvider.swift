import Foundation

struct CleanupProviderCapabilities: Equatable, Sendable {
    let supportedContextSources: Set<TranscriptContextSource>
    let supportsIncognito: Bool
    let hasDurableSideEffects: Bool

    static let localEphemeral = CleanupProviderCapabilities(
        supportedContextSources: Set([
            .application,
            .windowTitle,
            .selectedText,
            .nearbyText,
        ]),
        supportsIncognito: true,
        hasDurableSideEffects: false
    )

    static let cloudTranscriptOnly = CleanupProviderCapabilities(
        supportedContextSources: [],
        supportsIncognito: false,
        hasDurableSideEffects: false
    )
}

struct CleanupProviderDescriptor: Equatable, Sendable {
    let id: String
    let displayName: String
    let modelIdentifier: String
    let capabilities: CleanupProviderCapabilities
}

enum CleanupProviderFailureCategory: String, Codable, Equatable, Sendable {
    case cancelled
    case timeout
    case authentication
    case quota
    case safety
    case malformedOutput
    case outputTooLarge
    case provider
    case skippedCapacity
}

struct VoiceProfileSnapshot: Equatable, Sendable {
    let id: String
    let version: String
    let meaningPreservationRequired: Bool

    static let faithful = VoiceProfileSnapshot(
        id: "faithful-cleanup",
        version: "faithful-cleanup-v1",
        meaningPreservationRequired: true
    )
}

struct CleanupContextPolicy: Equatable, Sendable {
    let allowedSources: Set<TranscriptContextSource>
    let perSourceCharacterLimit: Int
    let totalCharacterLimit: Int
    let segmentCharacterLimit: Int
    let vocabularyItemLimit: Int
    let vocabularyItemCharacterLimit: Int
    let responseCharacterLimit: Int

    init(
        allowedSources: Set<TranscriptContextSource>,
        perSourceCharacterLimit: Int = 2_000,
        totalCharacterLimit: Int = 4_000,
        segmentCharacterLimit: Int = 8_000,
        vocabularyItemLimit: Int = 100,
        vocabularyItemCharacterLimit: Int = 128,
        responseCharacterLimit: Int = 8_000
    ) {
        self.allowedSources = allowedSources
        self.perSourceCharacterLimit = max(0, perSourceCharacterLimit)
        self.totalCharacterLimit = max(0, totalCharacterLimit)
        self.segmentCharacterLimit = max(1, segmentCharacterLimit)
        self.vocabularyItemLimit = max(0, vocabularyItemLimit)
        self.vocabularyItemCharacterLimit = max(1, vocabularyItemCharacterLimit)
        self.responseCharacterLimit = max(1, responseCharacterLimit)
    }

    func applying(to context: TranscriptContext) -> TranscriptContext {
        var remaining = totalCharacterLimit

        func bounded(_ value: String?, source: TranscriptContextSource) -> String? {
            guard allowedSources.contains(source),
                  remaining > 0,
                  let value
            else {
                return nil
            }
            let limit = min(perSourceCharacterLimit, remaining)
            let result = String(value.prefix(limit))
            remaining -= result.count
            return result.isEmpty ? nil : result
        }

        return TranscriptContext(
            applicationName: bounded(context.applicationName, source: .application),
            windowTitle: bounded(context.windowTitle, source: .windowTitle),
            selectedText: bounded(context.selectedText, source: .selectedText),
            nearbyText: bounded(context.nearbyText, source: .nearbyText)
        )
    }

    func boundedSegment(_ value: String) -> String {
        String(value.prefix(segmentCharacterLimit))
    }

    func boundedVocabulary(_ values: [String]) -> [String] {
        values.prefix(vocabularyItemLimit).compactMap { value in
            let bounded = String(value.prefix(vocabularyItemCharacterLimit))
            return bounded.isEmpty ? nil : bounded
        }
    }
}

struct CleanupRequest: Equatable, Sendable {
    let sessionID: String
    let segmentID: String
    let segmentSequence: Int
    let rawText: String
    let preparedText: String
    let context: TranscriptContext
    let protectedVocabulary: [String]
    let voiceProfile: VoiceProfileSnapshot
    let maximumResponseCharacters: Int
}

enum CleanupProviderResult: Equatable, Sendable {
    case success(String)
    case failure(CleanupProviderFailureCategory)
}

protocol CleanupProvider: Sendable {
    var descriptor: CleanupProviderDescriptor { get }
    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult
}

enum CleanupProviderRegistryError: Error, Equatable, LocalizedError {
    case duplicateProviderID(String)
    case unknownProviderID(String)
    case invalidProviderID(String)
    case unstableModelIdentifier(String)
    case unsupportedContext(String)
    case incognitoUnsupported(String)
    case incognitoDurabilityConflict(String)

    var errorDescription: String? {
        switch self {
        case .duplicateProviderID(let id):
            "Cleanup provider ID '\(id)' is registered more than once."
        case .unknownProviderID(let id):
            "Cleanup provider '\(id)' is not available."
        case .invalidProviderID(let id):
            "Cleanup provider ID '\(id)' is not a stable identifier."
        case .unstableModelIdentifier(let identifier):
            "Cleanup model '\(identifier)' is an unresolved alias."
        case .unsupportedContext(let id):
            "Cleanup provider '\(id)' cannot receive the selected context."
        case .incognitoUnsupported(let id):
            "Cleanup provider '\(id)' does not support Incognito sessions."
        case .incognitoDurabilityConflict(let id):
            "Cleanup provider '\(id)' may write durable state and is blocked in Incognito."
        }
    }
}

struct CleanupProviderRegistry: Sendable {
    private let providersByID: [String: any CleanupProvider]

    init(providers: [any CleanupProvider]) throws {
        var registered: [String: any CleanupProvider] = [:]
        for provider in providers {
            let descriptor = provider.descriptor
            guard Self.isStableProviderID(descriptor.id) else {
                throw CleanupProviderRegistryError.invalidProviderID(descriptor.id)
            }
            guard Self.isStableModelIdentifier(descriptor.modelIdentifier) else {
                throw CleanupProviderRegistryError.unstableModelIdentifier(
                    descriptor.modelIdentifier
                )
            }
            guard registered[descriptor.id] == nil else {
                throw CleanupProviderRegistryError.duplicateProviderID(descriptor.id)
            }
            registered[descriptor.id] = provider
        }
        providersByID = registered
    }

    func resolve(
        id: String,
        contextPolicy: CleanupContextPolicy,
        incognito: Bool
    ) throws -> any CleanupProvider {
        guard let provider = providersByID[id] else {
            throw CleanupProviderRegistryError.unknownProviderID(id)
        }
        let capabilities = provider.descriptor.capabilities
        guard contextPolicy.allowedSources.isSubset(
            of: capabilities.supportedContextSources
        ) else {
            throw CleanupProviderRegistryError.unsupportedContext(id)
        }
        if incognito && !capabilities.supportsIncognito {
            throw CleanupProviderRegistryError.incognitoUnsupported(id)
        }
        if incognito && capabilities.hasDurableSideEffects {
            throw CleanupProviderRegistryError.incognitoDurabilityConflict(id)
        }
        return provider
    }

    private static func isStableProviderID(_ value: String) -> Bool {
        guard !value.isEmpty else { return false }
        return value.allSatisfy {
            $0.isASCII && ($0.isLetter || $0.isNumber || $0 == "." || $0 == "-")
        }
    }

    private static func isStableModelIdentifier(_ value: String) -> Bool {
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
            .lowercased()
        guard normalized.count >= 5,
              normalized.contains(".") || normalized.contains("-")
        else {
            return false
        }
        let components = normalized.split(whereSeparator: { $0 == "." || $0 == "-" })
        let aliases: Set<Substring> = ["latest", "haiku", "sonnet", "opus"]
        return aliases.isDisjoint(with: components)
    }
}

struct AppleIntelligenceCleanupProvider: CleanupProvider {
    static let providerID = "apple-intelligence"
    static let modelIdentifier = "apple.foundation-model.on-device-v1"

    let descriptor = CleanupProviderDescriptor(
        id: providerID,
        displayName: "Apple Intelligence",
        modelIdentifier: modelIdentifier,
        capabilities: .localEphemeral
    )

    private let enhancer: any TranscriptEnhancing

    init(enhancer: any TranscriptEnhancing) {
        self.enhancer = enhancer
    }

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        guard !Task.isCancelled else { return .failure(.cancelled) }
        let result = await enhancer.enhance(TranscriptEnhancementRequest(
            text: request.preparedText,
            context: request.context,
            protectedVocabulary: request.protectedVocabulary
        ))
        guard !Task.isCancelled else { return .failure(.cancelled) }
        switch result.outcome {
        case .applied:
            let text = result.text.trimmingCharacters(in: .whitespacesAndNewlines)
            guard !text.isEmpty else { return .failure(.malformedOutput) }
            guard text.count <= request.maximumResponseCharacters else {
                return .failure(.outputTooLarge)
            }
            return .success(text)
        case .rejectedUnsafe:
            return .failure(.safety)
        case .notRequested, .unavailable:
            return .failure(.provider)
        }
    }
}

struct DisabledCleanupProvider: CleanupProvider {
    let descriptor = CleanupProviderDescriptor(
        id: "cleanup-disabled",
        displayName: "Cleanup off",
        modelIdentifier: "none.raw-transcript-v1",
        capabilities: .localEphemeral
    )

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        .success(request.preparedText)
    }
}
