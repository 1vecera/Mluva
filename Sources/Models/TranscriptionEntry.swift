import Foundation

enum TranscriptionMode: String, Codable, Sendable {
    case dictation
    case command
    case scratchpad
    case meeting
}

enum TranscriptionDeliveryOutcome: String, Codable, Sendable {
    case delivered
    case pendingDelivery
    case failed
}

struct TranscriptionTimings: Codable, Equatable, Sendable {
    static let empty = TranscriptionTimings()

    let captureLatency: TimeInterval?
    let recognitionLatency: TimeInterval?
    let enhancementLatency: TimeInterval?
    let deliveryLatency: TimeInterval?

    init(
        captureLatency: TimeInterval? = nil,
        recognitionLatency: TimeInterval? = nil,
        enhancementLatency: TimeInterval? = nil,
        deliveryLatency: TimeInterval? = nil
    ) {
        self.captureLatency = captureLatency.map { max(0, $0) }
        self.recognitionLatency = recognitionLatency.map { max(0, $0) }
        self.enhancementLatency = enhancementLatency.map { max(0, $0) }
        self.deliveryLatency = deliveryLatency.map { max(0, $0) }
    }

    func updatingDeliveryLatency(_ latency: TimeInterval) -> TranscriptionTimings {
        TranscriptionTimings(
            captureLatency: captureLatency,
            recognitionLatency: recognitionLatency,
            enhancementLatency: enhancementLatency,
            deliveryLatency: latency
        )
    }

    func updatingCaptureLatency(_ latency: TimeInterval) -> TranscriptionTimings {
        TranscriptionTimings(
            captureLatency: latency,
            recognitionLatency: recognitionLatency,
            enhancementLatency: enhancementLatency,
            deliveryLatency: deliveryLatency
        )
    }

    func updatingRecognitionLatency(_ latency: TimeInterval) -> TranscriptionTimings {
        TranscriptionTimings(
            captureLatency: captureLatency,
            recognitionLatency: latency,
            enhancementLatency: enhancementLatency,
            deliveryLatency: deliveryLatency
        )
    }

    func updatingEnhancementLatency(_ latency: TimeInterval) -> TranscriptionTimings {
        TranscriptionTimings(
            captureLatency: captureLatency,
            recognitionLatency: recognitionLatency,
            enhancementLatency: latency,
            deliveryLatency: deliveryLatency
        )
    }
}

enum ProviderFallbackReason: String, Codable, Equatable, Sendable {
    case providerStartupFailed
    case providerFinalizationFailed
}

struct ProviderFallbackEvent: Codable, Equatable, Sendable {
    let from: TranscriptionProviderKind
    let to: TranscriptionProviderKind
    let reason: ProviderFallbackReason
}

struct TranscriptionEntry: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let title: String?
    let rawText: String
    let deliveredText: String
    let correctionSourceText: String?
    let timestamp: Date
    let duration: TimeInterval
    let provider: TranscriptionProviderKind
    let language: String
    let mode: TranscriptionMode
    let targetApplicationName: String?
    let targetBundleIdentifier: String?
    let deliveryOutcome: TranscriptionDeliveryOutcome
    let failureMessage: String?
    let retainedAudioFilename: String?
    let enhancementOutcome: TranscriptEnhancementOutcome
    let cleanupProviderID: String?
    let cleanupModelIdentifier: String?
    let styleName: String?
    let styleOutcome: TranscriptStyleOutcome
    let contextSources: [TranscriptContextSource]
    let fallbackEvent: ProviderFallbackEvent?
    let timings: TranscriptionTimings

    var text: String { deliveredText }
    var canDeliverFromHistory: Bool {
        !deliveredText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    init(text: String, duration: TimeInterval = 0) {
        self.init(
            rawText: text,
            deliveredText: text,
            duration: duration,
            provider: .automatic,
            language: "auto",
            mode: .dictation,
            deliveryOutcome: .delivered
        )
    }

    init(
        id: UUID = UUID(),
        title: String? = nil,
        rawText: String,
        deliveredText: String,
        correctionSourceText: String? = nil,
        timestamp: Date = Date(),
        duration: TimeInterval = 0,
        provider: TranscriptionProviderKind,
        language: String,
        mode: TranscriptionMode,
        targetApplicationName: String? = nil,
        targetBundleIdentifier: String? = nil,
        deliveryOutcome: TranscriptionDeliveryOutcome,
        failureMessage: String? = nil,
        retainedAudioFilename: String? = nil,
        enhancementOutcome: TranscriptEnhancementOutcome = .notRequested,
        cleanupProviderID: String? = nil,
        cleanupModelIdentifier: String? = nil,
        styleName: String? = nil,
        styleOutcome: TranscriptStyleOutcome = .notRequested,
        contextSources: [TranscriptContextSource] = [],
        fallbackEvent: ProviderFallbackEvent? = nil,
        timings: TranscriptionTimings = .empty
    ) {
        self.id = id
        self.title = title
        self.rawText = rawText
        self.deliveredText = deliveredText
        self.correctionSourceText = correctionSourceText
        self.timestamp = timestamp
        self.duration = duration
        self.provider = provider
        self.language = language
        self.mode = mode
        self.targetApplicationName = targetApplicationName
        self.targetBundleIdentifier = targetBundleIdentifier
        self.deliveryOutcome = deliveryOutcome
        self.failureMessage = failureMessage
        self.retainedAudioFilename = retainedAudioFilename
        self.enhancementOutcome = enhancementOutcome
        self.cleanupProviderID = cleanupProviderID
        self.cleanupModelIdentifier = cleanupModelIdentifier
        self.styleName = styleName
        self.styleOutcome = styleOutcome
        self.contextSources = contextSources
        self.fallbackEvent = fallbackEvent
        self.timings = timings
    }

    func updatingDelivery(
        outcome: TranscriptionDeliveryOutcome,
        failureMessage: String? = nil,
        deliveryLatency: TimeInterval? = nil
    ) -> TranscriptionEntry {
        TranscriptionEntry(
            id: id,
            title: title,
            rawText: rawText,
            deliveredText: deliveredText,
            correctionSourceText: correctionSourceText,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: mode,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: outcome,
            failureMessage: failureMessage,
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: enhancementOutcome,
            cleanupProviderID: cleanupProviderID,
            cleanupModelIdentifier: cleanupModelIdentifier,
            styleName: styleName,
            styleOutcome: styleOutcome,
            contextSources: contextSources,
            fallbackEvent: fallbackEvent,
            timings: deliveryLatency.map(timings.updatingDeliveryLatency) ?? timings
        )
    }

    func renamed(_ title: String?) -> TranscriptionEntry {
        let normalizedTitle = title?
            .trimmingCharacters(in: .whitespacesAndNewlines)
        return TranscriptionEntry(
            id: id,
            title: normalizedTitle?.isEmpty == false ? normalizedTitle : nil,
            rawText: rawText,
            deliveredText: deliveredText,
            correctionSourceText: correctionSourceText,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: mode,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: deliveryOutcome,
            failureMessage: failureMessage,
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: enhancementOutcome,
            cleanupProviderID: cleanupProviderID,
            cleanupModelIdentifier: cleanupModelIdentifier,
            styleName: styleName,
            styleOutcome: styleOutcome,
            contextSources: contextSources,
            fallbackEvent: fallbackEvent,
            timings: timings
        )
    }

    func reprocessed(deliveredText: String) -> TranscriptionEntry {
        TranscriptionEntry(
            id: id,
            title: title,
            rawText: rawText,
            deliveredText: deliveredText,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: mode,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: .pendingDelivery,
            failureMessage: "Reprocessed. Paste the updated text when ready.",
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: .notRequested,
            fallbackEvent: fallbackEvent,
            timings: TranscriptionTimings(
                captureLatency: timings.captureLatency,
                recognitionLatency: timings.recognitionLatency
            )
        )
    }

    func restoringRawTranscript() -> TranscriptionEntry {
        let restoredText = rawText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !restoredText.isEmpty, restoredText != deliveredText else { return self }
        return TranscriptionEntry(
            id: id,
            title: title,
            rawText: self.rawText,
            deliveredText: restoredText,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: mode,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: .pendingDelivery,
            failureMessage: "Raw transcript restored. Paste it when ready.",
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: .notRequested,
            fallbackEvent: fallbackEvent,
            timings: TranscriptionTimings(
                captureLatency: timings.captureLatency,
                recognitionLatency: timings.recognitionLatency
            )
        )
    }

    func corrected(deliveredText: String) -> TranscriptionEntry {
        let deliveredText = deliveredText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !deliveredText.isEmpty, deliveredText != self.deliveredText else { return self }
        return TranscriptionEntry(
            id: id,
            title: title,
            rawText: rawText,
            deliveredText: deliveredText,
            correctionSourceText: correctionSourceText ?? self.deliveredText,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: mode,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: .pendingDelivery,
            failureMessage: "Edited. Paste the corrected text when ready.",
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: enhancementOutcome,
            cleanupProviderID: cleanupProviderID,
            cleanupModelIdentifier: cleanupModelIdentifier,
            styleName: styleName,
            styleOutcome: styleOutcome,
            contextSources: contextSources,
            fallbackEvent: fallbackEvent,
            timings: TranscriptionTimings(
                captureLatency: timings.captureLatency,
                recognitionLatency: timings.recognitionLatency,
                enhancementLatency: timings.enhancementLatency
            )
        )
    }

    func acceptingScratchpad(
        text: String,
        retainedAudioFilename: String?,
        styleName: String? = nil,
        styleOutcome: TranscriptStyleOutcome = .notRequested,
        contextSources: [TranscriptContextSource]? = nil,
        deliveryLatency: TimeInterval? = nil
    ) -> TranscriptionEntry {
        TranscriptionEntry(
            id: id,
            title: title,
            rawText: rawText,
            deliveredText: text,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            mode: .scratchpad,
            targetApplicationName: targetApplicationName,
            targetBundleIdentifier: targetBundleIdentifier,
            deliveryOutcome: .delivered,
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: enhancementOutcome,
            cleanupProviderID: cleanupProviderID,
            cleanupModelIdentifier: cleanupModelIdentifier,
            styleName: styleName,
            styleOutcome: styleOutcome,
            contextSources: contextSources ?? self.contextSources,
            fallbackEvent: fallbackEvent,
            timings: deliveryLatency.map(timings.updatingDeliveryLatency) ?? timings
        )
    }

    private enum CodingKeys: String, CodingKey {
        case id
        case title
        case text
        case rawText
        case deliveredText
        case correctionSourceText
        case timestamp
        case duration
        case provider
        case language
        case mode
        case targetApplicationName
        case targetBundleIdentifier
        case deliveryOutcome
        case failureMessage
        case retainedAudioFilename
        case enhancementOutcome
        case cleanupProviderID
        case cleanupModelIdentifier
        case styleName
        case styleOutcome
        case contextSources
        case fallbackEvent
        case timings
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        let legacyText = try container.decodeIfPresent(String.self, forKey: .text) ?? ""

        id = try container.decode(UUID.self, forKey: .id)
        title = try container.decodeIfPresent(String.self, forKey: .title)
        rawText = try container.decodeIfPresent(String.self, forKey: .rawText) ?? legacyText
        deliveredText = try container.decodeIfPresent(String.self, forKey: .deliveredText)
            ?? legacyText
        correctionSourceText = try container.decodeIfPresent(
            String.self,
            forKey: .correctionSourceText
        )
        timestamp = try container.decode(Date.self, forKey: .timestamp)
        duration = try container.decodeIfPresent(TimeInterval.self, forKey: .duration) ?? 0
        provider = try container.decodeIfPresent(
            TranscriptionProviderKind.self,
            forKey: .provider
        ) ?? .automatic
        language = try container.decodeIfPresent(String.self, forKey: .language) ?? "auto"
        mode = try container.decodeIfPresent(TranscriptionMode.self, forKey: .mode) ?? .dictation
        targetApplicationName = try container.decodeIfPresent(
            String.self,
            forKey: .targetApplicationName
        )
        targetBundleIdentifier = try container.decodeIfPresent(
            String.self,
            forKey: .targetBundleIdentifier
        )
        deliveryOutcome = try container.decodeIfPresent(
            TranscriptionDeliveryOutcome.self,
            forKey: .deliveryOutcome
        ) ?? .delivered
        failureMessage = try container.decodeIfPresent(String.self, forKey: .failureMessage)
        retainedAudioFilename = try container.decodeIfPresent(
            String.self,
            forKey: .retainedAudioFilename
        )
        enhancementOutcome = try container.decodeIfPresent(
            TranscriptEnhancementOutcome.self,
            forKey: .enhancementOutcome
        ) ?? .notRequested
        cleanupProviderID = try container.decodeIfPresent(
            String.self,
            forKey: .cleanupProviderID
        )
        cleanupModelIdentifier = try container.decodeIfPresent(
            String.self,
            forKey: .cleanupModelIdentifier
        )
        styleName = try container.decodeIfPresent(String.self, forKey: .styleName)
        styleOutcome = try container.decodeIfPresent(
            TranscriptStyleOutcome.self,
            forKey: .styleOutcome
        ) ?? .notRequested
        contextSources = try container.decodeIfPresent(
            [TranscriptContextSource].self,
            forKey: .contextSources
        ) ?? []
        fallbackEvent = try container.decodeIfPresent(
            ProviderFallbackEvent.self,
            forKey: .fallbackEvent
        )
        timings = try container.decodeIfPresent(
            TranscriptionTimings.self,
            forKey: .timings
        ) ?? .empty
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(id, forKey: .id)
        try container.encodeIfPresent(title, forKey: .title)
        try container.encode(deliveredText, forKey: .text)
        try container.encode(rawText, forKey: .rawText)
        try container.encode(deliveredText, forKey: .deliveredText)
        try container.encodeIfPresent(correctionSourceText, forKey: .correctionSourceText)
        try container.encode(timestamp, forKey: .timestamp)
        try container.encode(duration, forKey: .duration)
        try container.encode(provider, forKey: .provider)
        try container.encode(language, forKey: .language)
        try container.encode(mode, forKey: .mode)
        try container.encodeIfPresent(targetApplicationName, forKey: .targetApplicationName)
        try container.encodeIfPresent(targetBundleIdentifier, forKey: .targetBundleIdentifier)
        try container.encode(deliveryOutcome, forKey: .deliveryOutcome)
        try container.encodeIfPresent(failureMessage, forKey: .failureMessage)
        try container.encodeIfPresent(retainedAudioFilename, forKey: .retainedAudioFilename)
        try container.encode(enhancementOutcome, forKey: .enhancementOutcome)
        try container.encodeIfPresent(cleanupProviderID, forKey: .cleanupProviderID)
        try container.encodeIfPresent(cleanupModelIdentifier, forKey: .cleanupModelIdentifier)
        try container.encodeIfPresent(styleName, forKey: .styleName)
        try container.encode(styleOutcome, forKey: .styleOutcome)
        try container.encode(contextSources, forKey: .contextSources)
        try container.encodeIfPresent(fallbackEvent, forKey: .fallbackEvent)
        try container.encode(timings, forKey: .timings)
    }
}
