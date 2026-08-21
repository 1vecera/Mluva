import AppKit
import Foundation
import Combine

enum RecordingState: Equatable {
    case idle
    case starting
    case recording
    case stopping
}

private struct RecordingSessionSnapshot {
    let id: String
    let processingConfiguration: TranscriptProcessingConfiguration
    let cleanupConfiguration: CleanupSessionConfiguration
    let cleanupModelIdentifier: String
    let cleanupContext: TranscriptContext
    let incognito: Bool
    let audioRetentionPolicy: AudioRetentionPolicy
    let language: String
}

@MainActor
final class RecordingController: ObservableObject {
    @Published var state: RecordingState = .idle
    @Published var partialText: String = ""
    @Published var lastCommittedText: String = ""
    @Published var error: String?
    @Published private(set) var lastSessionResult: TranscriptionEntry?
    @Published private(set) var audioLevel = 0.0
    @Published private(set) var recordingStartedAt: Date?
    @Published private(set) var activeProviderKind: TranscriptionProviderKind?
    @Published private(set) var fallbackNotice: String?
    @Published private(set) var inputDeviceName: String?
    @Published private(set) var pendingCommandPreview: TranscriptCommandPreview?
    @Published private(set) var pendingScratchpadDraft: ScratchpadDraft?
    @Published private(set) var isScratchpadAcceptanceInProgress = false
    @Published private(set) var isScratchpadStyleProcessingInProgress = false
    @Published private(set) var activeCleanupProviderName: String?
    @Published private(set) var activeCleanupModelIdentifier: String?
    @Published private(set) var activeCleanupContextSources: [TranscriptContextSource] = []
    @Published private(set) var lastSessionWasIncognito = false

    private var audioCapture: (any AudioCapturing)?
    private var provider: (any TranscriptionProvider)?
    private let providerFactory: any TranscriptionProviderBuilding
    private let audioCaptureFactory: () -> any AudioCapturing
    private let destination: (any TextDestination)?
    private let makeSessionID: () -> String
    private let presentsFeedback: Bool
    private let captureTarget: () -> (any TextTargetRestoring)?
    private let personalizationStore: PersonalizationStore
    private let audioRetentionStore: AudioRetentionStore
    private let scratchpadDraftStore: ScratchpadDraftStore
    private let transcriptEnhancer: any TranscriptEnhancing
    private let cleanupProviderRegistry: CleanupProviderRegistry
    private let googleCloudRewriteProviderFactory: any GoogleCloudRewriteProviderBuilding
    private let transcriptCommander: any TranscriptCommanding
    private let transcriptStyler: any TranscriptStyling
    private let scratchpadClipboardWriter: (String) -> Bool
    private let retainedAudio = AudioCaptureBuffer()
    private let transcriptProcessor = TranscriptProcessor()
    private var transcriptAccumulator = TranscriptAccumulator()
    private var rawTranscriptAccumulator = TranscriptAccumulator()
    private var latestProviderText = ""
    private var cleanupSession: TranscriptCleanupSession?
    private var sessionSnapshot: RecordingSessionSnapshot?
    private var sessionID = ""
    private var stopRequested = false
    private var pendingStopCompletions: [(String) -> Void] = []
    private var stopTask: Task<Void, Never>?
    private var sessionTarget: (any TextTargetRestoring)?
    private var pendingCommandTarget: (any TextTargetRestoring)?
    private var pendingScratchpadTarget: (any TextTargetRestoring)?
    private var sessionStyle: SavedStyle?
    private var sessionTranscriptStyler: (any TranscriptStyling)?
    private var sessionTranscriptCommander: (any TranscriptCommanding)?
    private var sessionUsesCloudRewrite = false
    private var sessionUsesGeminiCommand = false
    private var sessionMode: TranscriptionMode = .dictation
    private var sessionProviderPreference: TranscriptionProviderKind = .automatic
    private var sessionFallbackEvent: ProviderFallbackEvent?
    private var sessionTimings = TranscriptionTimings.empty
    private var sessionContext = TranscriptContext.empty
    private var pendingScratchpadContext = TranscriptContext.empty
    let keyboardSimulator = KeyboardSimulator()
    private var startTime: Date?
    private var onStopped: (() -> Void)?
    let settings: AppSettings

    /// Closure that returns whether Accessibility is currently granted.
    /// Set by AppDelegate to provide live accessibility status.
    var accessibilityCheck: (() -> Bool)?

    init(
        settings: AppSettings = .shared,
        providerFactory: any TranscriptionProviderBuilding = DefaultTranscriptionProviderFactory(),
        audioCaptureFactory: @escaping () -> any AudioCapturing = { AudioCaptureService() },
        destination: (any TextDestination)? = nil,
        makeSessionID: @escaping () -> String = { UUID().uuidString },
        presentsFeedback: Bool = true,
        personalizationStore: PersonalizationStore = .shared,
        audioRetentionStore: AudioRetentionStore = .shared,
        scratchpadDraftStore: ScratchpadDraftStore = .shared,
        transcriptEnhancer: any TranscriptEnhancing = FaithfulTranscriptEnhancer(
            backend: SystemFoundationModelEnhancementBackend()
        ),
        cleanupProviderRegistry: CleanupProviderRegistry? = nil,
        googleCloudRewriteProviderFactory: any GoogleCloudRewriteProviderBuilding = DefaultGoogleCloudRewriteProviderFactory(),
        transcriptCommander: any TranscriptCommanding = SystemFoundationModelTranscriptCommander(),
        transcriptStyler: any TranscriptStyling = FaithfulTranscriptStyler(
            backend: SystemFoundationModelStyleBackend()
        ),
        scratchpadClipboardWriter: @escaping (String) -> Bool = { text in
            let pasteboard = NSPasteboard.general
            pasteboard.clearContents()
            return pasteboard.setString(text, forType: .string)
        },
        captureTarget: (() -> (any TextTargetRestoring)?)? = nil
    ) {
        self.settings = settings
        self.providerFactory = providerFactory
        self.audioCaptureFactory = audioCaptureFactory
        self.destination = destination
        self.makeSessionID = makeSessionID
        self.presentsFeedback = presentsFeedback
        self.personalizationStore = personalizationStore
        self.audioRetentionStore = audioRetentionStore
        self.scratchpadDraftStore = scratchpadDraftStore
        self.transcriptEnhancer = transcriptEnhancer
        self.cleanupProviderRegistry = cleanupProviderRegistry ?? (try! CleanupProviderRegistry(
            providers: [AppleIntelligenceCleanupProvider(enhancer: transcriptEnhancer)]
        ))
        self.googleCloudRewriteProviderFactory = googleCloudRewriteProviderFactory
        self.transcriptCommander = transcriptCommander
        self.transcriptStyler = transcriptStyler
        self.scratchpadClipboardWriter = scratchpadClipboardWriter
        self.captureTarget = captureTarget ?? {
            ApplicationFocusTracker.shared.captureTarget(
                captureSelectedText: !settings.rememberLastModePerApplication
                    && settings.transcriptionMode == .command,
                captureSelectedTextForBundleIdentifier: { bundleIdentifier in
                    personalizationStore.selectedMode(
                        for: bundleIdentifier,
                        rememberPerApplication: settings.rememberLastModePerApplication,
                        fallback: settings.transcriptionMode
                    ) == .command
                },
                contextAllowed: settings.contextualFormattingAllowed(for:)
            )
        }
        pendingScratchpadDraft = scratchpadDraftStore.draft
        lastSessionResult = scratchpadDraftStore.draft?.entry
        if let restoredEntry = scratchpadDraftStore.draft?.entry,
           restoredEntry.targetBundleIdentifier != nil
            || restoredEntry.targetApplicationName != nil {
            pendingScratchpadTarget = StoredApplicationTarget(
                bundleIdentifier: restoredEntry.targetBundleIdentifier,
                applicationName: restoredEntry.targetApplicationName
            )
        }
    }

    private let fillerPattern = try! NSRegularExpression(
        pattern: #"\b(uh huh|um|uh|hmm|mhm|mm)\b"#,
        options: .caseInsensitive
    )

    // MARK: - Start

    func start(onStopped: @escaping () -> Void) {
        guard state == .idle else { return }
        guard pendingCommandPreview == nil else {
            error = "Apply or discard the command preview before recording again."
            return
        }
        guard pendingScratchpadDraft == nil else {
            error = "Copy, insert, or delete the scratchpad before recording again."
            return
        }
        state = .starting
        let captureRequestedAt = Date()
        self.onStopped = onStopped
        error = nil
        lastSessionResult = nil
        stopRequested = false
        pendingStopCompletions = []
        stopTask?.cancel()
        stopTask = nil
        sessionID = makeSessionID()
        sessionTimings = .empty
        sessionFallbackEvent = nil
        fallbackNotice = nil
        let activeSessionID = sessionID
        sessionTarget = captureTarget()
        sessionMode = personalizationStore.selectedMode(
            for: sessionTarget?.targetBundleIdentifier,
            rememberPerApplication: settings.rememberLastModePerApplication,
            fallback: settings.transcriptionMode
        )
        guard sessionMode != .meeting else {
            state = .idle
            error = "Start meetings from the explicit Meeting mode controls."
            sessionTarget = nil
            sessionID = ""
            notifyStopped()
            return
        }
        sessionProviderPreference = personalizationStore.selectedProvider(
            for: sessionTarget?.targetBundleIdentifier,
            rememberPerApplication: settings.rememberProviderPerApplication,
            fallback: settings.providerPreference
        )
        sessionContext = settings.contextualFormattingAllowed(
            for: sessionTarget?.targetBundleIdentifier
        ) ? sessionTarget?.transcriptContext ?? .empty : .empty
        sessionStyle = personalizationStore.selectedStyle(
            for: sessionTarget?.targetBundleIdentifier,
            rememberPerApplication: settings.rememberLastStylePerApplication
        )
        let processingConfiguration = personalizationStore.processingConfiguration(
            removeFillers: settings.removeFiller,
            targetBundleIdentifier: sessionTarget?.targetBundleIdentifier
        )
        lastSessionWasIncognito = settings.incognitoMode
        sessionTranscriptStyler = nil
        sessionTranscriptCommander = nil
        sessionUsesCloudRewrite = false
        sessionUsesGeminiCommand = false
        cleanupSession = nil
        sessionSnapshot = nil
        transcriptAccumulator = TranscriptAccumulator()
        rawTranscriptAccumulator = TranscriptAccumulator()
        latestProviderText = ""
        retainedAudio.reset()
        audioLevel = 0
        recordingStartedAt = nil
        activeProviderKind = nil
        inputDeviceName = nil

        if presentsFeedback {
            FloatingTranscriptWindow.shared.showPreparing(
                provider: sessionProviderPreference
            )
        }

        Task { @MainActor in
            do {
                var activeProvider = try providerFactory.makeProvider(
                    settings: settings,
                    preference: sessionProviderPreference
                )
                configureProvider(activeProvider, sessionID: activeSessionID)
                self.provider = activeProvider
                self.activeProviderKind = activeProvider.kind
                if presentsFeedback {
                    FloatingTranscriptWindow.shared.updatePreparing(
                        provider: activeProvider.kind
                    )
                }
                do {
                    try await activeProvider.start()
                } catch {
                    guard let fallback = try await activateFallbackProvider(
                        from: activeProvider,
                        reason: .providerStartupFailed
                    ) else {
                        throw error
                    }
                    activeProvider = fallback
                }

                guard sessionID == activeSessionID else {
                    activeProvider.cancel()
                    return
                }

                if stopRequested {
                    activeProvider.cancel()
                    finishCancelledStart()
                    return
                }

                try prepareRewritePipeline(
                    processingConfiguration: processingConfiguration,
                    providerKind: activeProvider.kind,
                    sessionID: activeSessionID
                )

                let capture = audioCaptureFactory()
                capture.onAudioChunk = { [weak self] chunk in
                    self?.retainedAudio.append(chunk)
                    self?.provider?.appendAudio(chunk)
                    let level = AudioLevelMeter.level(for: chunk)
                    DispatchQueue.main.async { [weak self] in
                        guard self?.sessionID == activeSessionID else { return }
                        self?.audioLevel = level
                        FloatingTranscriptWindow.shared.updateAudioLevel(level)
                    }
                }
                try capture.start()
                self.audioCapture = capture
                self.sessionTimings = self.sessionTimings.updatingCaptureLatency(
                    Date().timeIntervalSince(captureRequestedAt)
                )

                if stopRequested {
                    capture.stop()
                    activeProvider.cancel()
                    finishCancelledStart()
                    return
                }

                self.startTime = Date()
                self.recordingStartedAt = self.startTime
                self.inputDeviceName = AudioCaptureService.currentInputDeviceName
                self.state = .recording

                if presentsFeedback {
                    NSSound(named: "Tink")?.play()
                    FloatingTranscriptWindow.shared.showReady(
                        provider: activeProvider.kind,
                        cleanupProviderName: activeCleanupProviderName ?? "",
                        cleanupModelIdentifier: activeCleanupModelIdentifier ?? "",
                        disclosedContextSources: activeCleanupContextSources,
                        inputDeviceName: inputDeviceName,
                        startedAt: startTime
                    )
                }

            } catch {
                guard sessionID == activeSessionID else { return }
                if stopRequested {
                    finishCancelledStart()
                    return
                }
                self.state = .idle
                self.error = error.localizedDescription
                self.provider?.cancel()
                self.provider = nil
                self.cleanupSession?.cancel()
                self.cleanupSession = nil
                self.sessionSnapshot = nil
                self.activeCleanupProviderName = nil
                self.activeCleanupModelIdentifier = nil
                self.activeCleanupContextSources = []
                if presentsFeedback {
                    FloatingTranscriptWindow.shared.hide()
                }
                notifyStopped()
            }
        }
    }

    // MARK: - Stop

    func stop(completion: @escaping (String) -> Void) {
        if state == .stopping {
            pendingStopCompletions.append(completion)
            return
        }
        if state == .starting {
            pendingStopCompletions.append(completion)
            stopRequested = true
            state = .stopping
            provider?.cancel()
            return
        }
        guard state == .recording else { return }
        pendingStopCompletions.append(completion)
        state = .stopping
        let recognitionStartedAt = Date()
        let activeSessionID = sessionID

        if presentsFeedback {
            FloatingTranscriptWindow.shared.showFinishing()
        }

        stopTask = Task { @MainActor in
            audioCapture?.stop()

            do {
                let providerTranscript: String
                do {
                    providerTranscript = try await provider?.finish() ?? ""
                } catch {
                    if let failedProvider = provider,
                       let fallback = try await activateFallbackProvider(
                           from: failedProvider,
                           reason: .providerFinalizationFailed
                       ) {
                        fallback.appendAudio(retainedAudio.snapshot())
                        providerTranscript = try await fallback.finish()
                    } else if !rawTranscriptAccumulator.finalText
                        .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        || !latestProviderText
                            .trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
                        providerTranscript = ""
                    } else {
                        throw error
                    }
                }
                sessionTimings = sessionTimings.updatingRecognitionLatency(
                    Date().timeIntervalSince(recognitionStartedAt)
                )
                guard sessionID == activeSessionID,
                      !Task.isCancelled
                else {
                    return
                }
                let enhancementStartedAt = Date()
                let finalAvailableText = [
                    rawTranscriptAccumulator.finalText,
                    providerTranscript,
                    latestProviderText,
                ].first {
                    !$0.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                } ?? ""
                let providerFinishSuffix = TranscriptAccumulator.nonOverlappingSuffix(
                    existing: rawTranscriptAccumulator.finalText,
                    incoming: providerTranscript
                )
                let finalSegmentText = cleanupSession?.hasStableSegments == true
                    ? providerFinishSuffix
                    : finalAvailableText
                if !finalSegmentText.isEmpty,
                   let snapshot = sessionSnapshot {
                    let processedFinal = transcriptProcessor.process(
                        finalSegmentText,
                        configuration: snapshot.processingConfiguration
                    )
                    cleanupSession?.acceptStableSegment(
                        id: "\(activeSessionID)-provider-finish",
                        rawText: finalSegmentText,
                        preparedText: processedFinal.text
                    )
                }
                let cleanupTerminal = await cleanupSession?.stopAndDrain()
                guard sessionID == activeSessionID,
                      !Task.isCancelled
                else {
                    return
                }
                let rawTranscript = cleanupTerminal?.rawText.isEmpty == false
                    ? cleanupTerminal?.rawText ?? finalAvailableText
                    : finalAvailableText
                let processingConfiguration = sessionSnapshot?.processingConfiguration
                    ?? personalizationStore.processingConfiguration(
                        removeFillers: settings.removeFiller,
                        targetBundleIdentifier: sessionTarget?.targetBundleIdentifier
                    )
                let processed = transcriptProcessor.process(
                    rawTranscript,
                    configuration: processingConfiguration
                )
                if sessionMode == .command,
                   !processed.text.isEmpty {
                    let proposedText = try await (
                        sessionTranscriptCommander ?? transcriptCommander
                    ).execute(
                        TranscriptCommandRequest(
                            instruction: processed.text,
                            sourceText: sessionTarget?.selectedText,
                            context: sessionUsesGeminiCommand ? .empty : sessionContext
                        )
                    ).trimmingCharacters(in: .whitespacesAndNewlines)
                    guard !proposedText.isEmpty else {
                        throw SystemTranscriptEnhancementError.unavailable
                    }
                    pendingCommandPreview = TranscriptCommandPreview(
                        deliveryID: sessionID,
                        instruction: processed.text,
                        sourceText: sessionTarget?.selectedText,
                        proposedText: proposedText
                    )
                    pendingCommandTarget = sessionTarget
                    sessionTimings = sessionTimings.updatingEnhancementLatency(
                        Date().timeIntervalSince(enhancementStartedAt)
                    )
                    lastSessionResult = makeSessionResult(
                        rawText: rawTranscript,
                        deliveredText: proposedText,
                        provider: provider?.kind ?? .automatic,
                        outcome: .pendingDelivery,
                        failureMessage: "Review the command preview before applying it.",
                        contextSources: commandContextSources
                    )
                    completeStop(transcript: proposedText, completion: completion)
                    return
                }
                let enhancement = if sessionSnapshot?.cleanupConfiguration.enabled == true,
                                     let cleanupTerminal {
                    TranscriptEnhancementResult(
                        text: cleanupTerminal.selectedText,
                        outcome: cleanupTerminal.enhancementOutcome,
                        violations: []
                    )
                } else {
                    TranscriptEnhancementResult(
                        text: processed.text,
                        outcome: .notRequested,
                        violations: []
                    )
                }
                let styling = if sessionMode == .dictation,
                                 let sessionStyle,
                                 !enhancement.text.isEmpty {
                    await (sessionTranscriptStyler ?? transcriptStyler).apply(TranscriptStyleRequest(
                        text: enhancement.text,
                        style: sessionStyle,
                        context: sessionContext,
                        protectedVocabulary: personalizationStore.recognitionContext
                    ))
                } else {
                    TranscriptStyleResult(
                        text: enhancement.text,
                        outcome: .notRequested,
                        violations: []
                    )
                }
                let finalText = styling.text
                sessionTimings = sessionTimings.updatingEnhancementLatency(
                    Date().timeIntervalSince(enhancementStartedAt)
                )

                if sessionMode == .scratchpad {
                    let result = makeSessionResult(
                        rawText: rawTranscript,
                        deliveredText: finalText,
                        provider: provider?.kind ?? .automatic,
                        outcome: .pendingDelivery,
                        failureMessage: "Review the scratchpad before copying or inserting it.",
                        enhancementOutcome: enhancement.outcome,
                        contextSources: sessionSnapshot?.cleanupConfiguration.enabled == true
                            ? sessionContext.sources
                            : [],
                        forceRetainAudio: true
                    )
                    let draft = ScratchpadDraft(
                        entry: result,
                        text: finalText,
                        deliveryID: sessionID,
                        selectedStyleID: sessionStyle?.id
                    )
                    lastSessionResult = result
                    pendingScratchpadDraft = draft
                    pendingScratchpadTarget = sessionTarget
                    pendingScratchpadContext = sessionContext
                    scratchpadDraftStore.save(
                        draft,
                        persist: !(sessionSnapshot?.incognito ?? settings.incognitoMode)
                    )
                    completeStop(transcript: finalText, completion: completion)
                    return
                }

                if !finalText.isEmpty {
                    let deliveryStartedAt = Date()
                    do {
                        let destination = self.destination ?? KeyboardTextDestination(
                            keyboardSimulator: keyboardSimulator,
                            canPaste: { [weak self] in self?.accessibilityCheck?() ?? false },
                            target: sessionTarget
                        )
                        let coordinator = DeliveryCoordinator(destination: destination)
                        _ = try await coordinator.deliver(
                            .final(id: sessionID, text: finalText)
                        )
                        sessionTimings = sessionTimings.updatingDeliveryLatency(
                            Date().timeIntervalSince(deliveryStartedAt)
                        )
                        lastCommittedText = finalText
                    } catch {
                        sessionTimings = sessionTimings.updatingDeliveryLatency(
                            Date().timeIntervalSince(deliveryStartedAt)
                        )
                        self.error = error.localizedDescription
                        lastSessionResult = makeSessionResult(
                            rawText: rawTranscript,
                            deliveredText: finalText,
                            provider: provider?.kind ?? .automatic,
                            outcome: .pendingDelivery,
                            failureMessage: error.localizedDescription,
                            enhancementOutcome: enhancement.outcome,
                            styleName: sessionStyle?.name,
                            styleOutcome: styling.outcome,
                            contextSources: rewriteContextSources
                        )
                        completeStop(transcript: finalText, completion: completion)
                        return
                    }
                }

                lastSessionResult = makeSessionResult(
                    rawText: rawTranscript,
                    deliveredText: finalText,
                    provider: provider?.kind ?? .automatic,
                    outcome: .delivered,
                    enhancementOutcome: enhancement.outcome,
                    styleName: sessionStyle?.name,
                    styleOutcome: styling.outcome,
                    contextSources: rewriteContextSources
                )

                if presentsFeedback {
                    NSSound(named: "Pop")?.play()
                }
                completeStop(transcript: finalText, completion: completion)
            } catch {
                guard sessionID == activeSessionID else { return }
                self.error = error.localizedDescription
                lastSessionResult = makeSessionResult(
                    rawText: rawTranscriptAccumulator.finalText.isEmpty
                        ? latestProviderText
                        : rawTranscriptAccumulator.finalText,
                    deliveredText: "",
                    provider: provider?.kind ?? .automatic,
                    outcome: .failed,
                    failureMessage: error.localizedDescription,
                    contextSources: contextSourcesForCurrentMode
                )
                completeStop(transcript: "", completion: completion)
            }
        }
    }

    func cancel() {
        guard state != .idle else { return }

        cleanupSession?.cancel()
        stopTask?.cancel()
        stopTask = nil
        sessionID = ""
        stopRequested = false
        let stopCompletions = pendingStopCompletions
        pendingStopCompletions.removeAll()
        audioCapture?.stop()
        provider?.cancel()
        audioCapture = nil
        provider = nil
        cleanupSession = nil
        sessionSnapshot = nil
        sessionTarget = nil
        sessionStyle = nil
        sessionTranscriptStyler = nil
        sessionTranscriptCommander = nil
        sessionUsesCloudRewrite = false
        sessionUsesGeminiCommand = false
        sessionContext = .empty
        startTime = nil
        recordingStartedAt = nil
        audioLevel = 0
        activeProviderKind = nil
        fallbackNotice = nil
        sessionFallbackEvent = nil
        inputDeviceName = nil
        activeCleanupProviderName = nil
        activeCleanupModelIdentifier = nil
        activeCleanupContextSources = []
        state = .idle
        partialText = ""
        if presentsFeedback {
            FloatingTranscriptWindow.shared.hide()
        }
        stopCompletions.forEach { $0("") }
        notifyStopped()
    }

    func acceptCommandPreview() async throws -> TranscriptionEntry? {
        guard let preview = pendingCommandPreview else { return nil }
        let deliveryStartedAt = Date()
        let destination = self.destination ?? KeyboardTextDestination(
            keyboardSimulator: keyboardSimulator,
            canPaste: { [weak self] in self?.accessibilityCheck?() ?? false },
            target: pendingCommandTarget
        )
        let coordinator = DeliveryCoordinator(destination: destination)

        do {
            _ = try await coordinator.deliver(
                .final(id: preview.deliveryID, text: preview.proposedText)
            )
            lastCommittedText = preview.proposedText
            lastSessionResult = lastSessionResult?.updatingDelivery(
                outcome: .delivered,
                deliveryLatency: Date().timeIntervalSince(deliveryStartedAt)
            )
            pendingCommandPreview = nil
            pendingCommandTarget = nil
            return lastSessionResult
        } catch {
            lastSessionResult = lastSessionResult?.updatingDelivery(
                outcome: .pendingDelivery,
                failureMessage: error.localizedDescription,
                deliveryLatency: Date().timeIntervalSince(deliveryStartedAt)
            )
            self.error = error.localizedDescription
            throw error
        }
    }

    func discardCommandPreview() {
        pendingCommandPreview = nil
        pendingCommandTarget = nil
        lastSessionResult = nil
        error = nil
    }

    func updateScratchpadText(_ text: String) {
        guard !isScratchpadAcceptanceInProgress,
              !isScratchpadStyleProcessingInProgress,
              var draft = pendingScratchpadDraft
        else {
            return
        }
        draft.text = text
        pendingScratchpadDraft = draft
        scratchpadDraftStore.save(
            draft,
            persist: !lastSessionWasIncognito
        )
    }

    func updateScratchpadStyle(_ id: UUID?) {
        guard !isScratchpadAcceptanceInProgress,
              !isScratchpadStyleProcessingInProgress,
              var draft = pendingScratchpadDraft
        else {
            return
        }
        let styleID = personalizationStore.style(id: id)?.id
        if draft.selectedStyleID != styleID {
            draft.appliedStyleName = nil
            draft.styleOutcome = .notRequested
        }
        draft.selectedStyleID = styleID
        pendingScratchpadDraft = draft
        personalizationStore.selectStyle(
            styleID,
            for: draft.entry.targetBundleIdentifier,
            rememberPerApplication: settings.rememberLastStylePerApplication
        )
        scratchpadDraftStore.save(
            draft,
            persist: !lastSessionWasIncognito
        )
    }

    @MainActor
    func applyStyleToScratchpadDraft() async throws -> ScratchpadDraft? {
        guard !isScratchpadAcceptanceInProgress,
              !isScratchpadStyleProcessingInProgress
        else {
            return nil
        }
        guard var draft = pendingScratchpadDraft else { return nil }
        guard let style = personalizationStore.style(id: draft.selectedStyleID) else {
            let error = ScratchpadDraftError.noStyleSelected
            self.error = error.localizedDescription
            throw error
        }

        isScratchpadStyleProcessingInProgress = true
        defer { isScratchpadStyleProcessingInProgress = false }
        let draftStyler = draft.entry.provider == .googleCloud
            && !lastSessionWasIncognito
            ? googleCloudRewriteProviderFactory.makeProviders(settings: settings).transcriptStyler
            : transcriptStyler
        let result = await draftStyler.apply(TranscriptStyleRequest(
            text: draft.text,
            style: style,
            context: pendingScratchpadContext,
            protectedVocabulary: personalizationStore.recognitionContext
        ))

        let styleError: ScratchpadDraftError?
        switch result.outcome {
        case .applied:
            styleError = nil
        case .rejectedUnsafe:
            styleError = .styleRejected
        case .notRequested, .unavailable:
            styleError = .styleUnavailable
        }
        if let styleError {
            self.error = styleError.localizedDescription
            throw styleError
        }

        draft.text = result.text
        draft.appliedStyleName = style.name
        draft.styleOutcome = result.outcome
        pendingScratchpadDraft = draft
        scratchpadDraftStore.save(
            draft,
            persist: !lastSessionWasIncognito
        )
        error = nil
        return draft
    }

    @MainActor
    func acceptScratchpadDraft(
        destination scratchpadDestination: ScratchpadDraftDestination
    ) async throws -> TranscriptionEntry? {
        guard !isScratchpadAcceptanceInProgress,
              !isScratchpadStyleProcessingInProgress
        else {
            return nil
        }
        guard let draft = pendingScratchpadDraft else { return nil }
        let deliveryStartedAt = Date()
        isScratchpadAcceptanceInProgress = true
        defer { isScratchpadAcceptanceInProgress = false }

        let text = draft.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else {
            let error = ScratchpadDraftError.empty
            self.error = error.localizedDescription
            throw error
        }

        do {
            switch scratchpadDestination {
            case .clipboard:
                guard scratchpadClipboardWriter(text) else {
                    throw ScratchpadDraftError.clipboardUnavailable
                }

            case .originalApplication:
                let textDestination: any TextDestination
                if let destination {
                    textDestination = destination
                } else {
                    guard let pendingScratchpadTarget,
                          accessibilityCheck?() == true
                    else {
                        throw ScratchpadDraftError.originalApplicationUnavailable
                    }
                    textDestination = KeyboardTextDestination(
                        keyboardSimulator: keyboardSimulator,
                        canPaste: { true },
                        target: pendingScratchpadTarget
                    )
                }
                let coordinator = DeliveryCoordinator(destination: textDestination)
                _ = try await coordinator.deliver(
                    .final(id: draft.deliveryID, text: text)
                )
            }
        } catch {
            self.error = error.localizedDescription
            throw error
        }

        let retainedAudioFilename: String?
        if settings.audioRetentionPolicy == .always {
            retainedAudioFilename = draft.entry.retainedAudioFilename
        } else {
            if let filename = draft.entry.retainedAudioFilename {
                try? audioRetentionStore.delete(filename: filename)
            }
            retainedAudioFilename = nil
        }
        let accepted = draft.entry.acceptingScratchpad(
            text: text,
            retainedAudioFilename: retainedAudioFilename,
            styleName: draft.appliedStyleName,
            styleOutcome: draft.styleOutcome,
            contextSources: mergedContextSources(
                draft.entry.contextSources,
                draft.styleOutcome == .applied
                    ? pendingScratchpadContext.sources
                    : []
            ),
            deliveryLatency: Date().timeIntervalSince(deliveryStartedAt)
        )
        pendingScratchpadDraft = nil
        pendingScratchpadTarget = nil
        pendingScratchpadContext = .empty
        scratchpadDraftStore.clear()
        lastSessionResult = accepted
        lastCommittedText = text
        error = nil
        return accepted
    }

    func discardScratchpadDraft() {
        guard !isScratchpadAcceptanceInProgress,
              !isScratchpadStyleProcessingInProgress
        else {
            return
        }
        guard let draft = pendingScratchpadDraft else { return }
        if let filename = draft.entry.retainedAudioFilename {
            try? audioRetentionStore.delete(filename: filename)
        }
        pendingScratchpadDraft = nil
        pendingScratchpadTarget = nil
        pendingScratchpadContext = .empty
        scratchpadDraftStore.clear()
        lastSessionResult = nil
        error = nil
    }

    private func receiveProviderEvent(
        _ event: TranscriptEvent,
        sessionID activeSessionID: String,
        providerIdentity: ObjectIdentifier
    ) {
        guard sessionID == activeSessionID,
              provider.map(ObjectIdentifier.init) == providerIdentity,
              state == .recording || state == .stopping
        else {
            return
        }
        latestProviderText = event.text
        rawTranscriptAccumulator.ingest(event)
        let configuration = sessionSnapshot?.processingConfiguration
            ?? personalizationStore.processingConfiguration(
                removeFillers: settings.removeFiller,
                targetBundleIdentifier: sessionTarget?.targetBundleIdentifier
            )
        let processed = transcriptProcessor.process(
            event.text,
            configuration: configuration
        )
        let cleanedEvent = TranscriptEvent(
            id: event.id,
            text: processed.text,
            kind: event.kind
        )
        transcriptAccumulator.ingest(cleanedEvent)
        partialText = processed.text
        switch event.kind {
        case .volatile:
            cleanupSession?.updateVolatileText(event.text)
        case .final:
            cleanupSession?.acceptStableSegment(
                id: event.id,
                rawText: event.text,
                preparedText: processed.text
            )
        }
        if presentsFeedback, cleanupSession == nil {
            FloatingTranscriptWindow.shared.updateText(processed.text)
        }
    }

    private func completeStop(
        transcript: String,
        completion: @escaping (String) -> Void
    ) {
        if presentsFeedback {
            FloatingTranscriptWindow.shared.hide()
        }
        state = .idle
        partialText = ""
        audioCapture = nil
        provider = nil
        sessionTarget = nil
        sessionStyle = nil
        sessionTranscriptStyler = nil
        sessionTranscriptCommander = nil
        sessionUsesCloudRewrite = false
        sessionUsesGeminiCommand = false
        sessionContext = .empty
        sessionID = ""
        startTime = nil
        recordingStartedAt = nil
        audioLevel = 0
        activeProviderKind = nil
        activeCleanupProviderName = nil
        activeCleanupModelIdentifier = nil
        activeCleanupContextSources = []
        inputDeviceName = nil
        stopRequested = false
        cleanupSession = nil
        sessionSnapshot = nil
        stopTask = nil
        let completions = pendingStopCompletions
        pendingStopCompletions.removeAll()
        if completions.isEmpty {
            completion(transcript)
        } else {
            completions.forEach { $0(transcript) }
        }
        notifyStopped()
    }

    private func makeSessionResult(
        rawText: String,
        deliveredText: String,
        provider: TranscriptionProviderKind,
        outcome: TranscriptionDeliveryOutcome,
        failureMessage: String? = nil,
        enhancementOutcome: TranscriptEnhancementOutcome = .notRequested,
        styleName: String? = nil,
        styleOutcome: TranscriptStyleOutcome = .notRequested,
        contextSources: [TranscriptContextSource] = [],
        forceRetainAudio: Bool = false
    ) -> TranscriptionEntry {
        let startedAt = startTime ?? Date()
        let entryID = UUID()
        let audio = retainedAudio.snapshot()
        let incognito = sessionSnapshot?.incognito ?? settings.incognitoMode
        let retentionPolicy = sessionSnapshot?.audioRetentionPolicy
            ?? settings.audioRetentionPolicy
        let shouldRetain = !incognito
            && !audio.isEmpty
            && (forceRetainAudio
                || retentionPolicy.shouldRetain(
                    deliverySucceeded: outcome == .delivered
                ))
        let retainedAudioFilename = shouldRetain
            ? try? audioRetentionStore.save(audio, for: entryID)
            : nil
        return TranscriptionEntry(
            id: entryID,
            rawText: rawText,
            deliveredText: deliveredText,
            timestamp: startedAt,
            duration: max(0, Date().timeIntervalSince(startedAt)),
            provider: provider,
            language: sessionSnapshot?.language ?? settings.language,
            mode: sessionMode,
            targetApplicationName: sessionTarget?.targetApplicationName,
            targetBundleIdentifier: sessionTarget?.targetBundleIdentifier,
            deliveryOutcome: outcome,
            failureMessage: failureMessage,
            retainedAudioFilename: retainedAudioFilename,
            enhancementOutcome: enhancementOutcome,
            cleanupProviderID: sessionSnapshot?.cleanupConfiguration.enabled == true
                ? sessionSnapshot?.cleanupConfiguration.providerID
                : nil,
            cleanupModelIdentifier: sessionSnapshot?.cleanupConfiguration.enabled == true
                ? sessionSnapshot?.cleanupModelIdentifier
                : nil,
            styleName: styleName,
            styleOutcome: styleOutcome,
            contextSources: contextSources,
            fallbackEvent: sessionFallbackEvent,
            timings: sessionTimings
        )
    }

    private func configureProvider(
        _ provider: any TranscriptionProvider,
        sessionID activeSessionID: String
    ) {
        let providerIdentity = ObjectIdentifier(provider)
        provider.onEvent = { [weak self] event in
            DispatchQueue.main.async {
                self?.receiveProviderEvent(
                    event,
                    sessionID: activeSessionID,
                    providerIdentity: providerIdentity
                )
            }
        }
    }

    @MainActor
    private func activateFallbackProvider(
        from failedProvider: any TranscriptionProvider,
        reason: ProviderFallbackReason
    ) async throws -> (any TranscriptionProvider)? {
        guard sessionProviderPreference == .automatic,
              sessionFallbackEvent == nil,
              let fallback = try providerFactory.makeFallbackProvider(
                  settings: settings,
                  excluding: failedProvider.kind
              )
        else {
            return nil
        }

        failedProvider.cancel()
        configureProvider(fallback, sessionID: sessionID)
        provider = fallback
        activeProviderKind = fallback.kind
        try await fallback.start()
        sessionFallbackEvent = ProviderFallbackEvent(
            from: failedProvider.kind,
            to: fallback.kind,
            reason: reason
        )
        fallbackNotice = "\(failedProvider.kind.displayName) unavailable · switched to \(fallback.kind.displayName)"
        if presentsFeedback {
            if state == .starting {
                FloatingTranscriptWindow.shared.updatePreparing(provider: fallback.kind)
            } else {
                FloatingTranscriptWindow.shared.updateProvider(fallback.kind)
                FloatingTranscriptWindow.shared.updateText(fallbackNotice ?? "")
            }
        }
        return fallback
    }

    private func prepareRewritePipeline(
        processingConfiguration: TranscriptProcessingConfiguration,
        providerKind: TranscriptionProviderKind,
        sessionID activeSessionID: String
    ) throws {
        let cloudRewriteAllowed = providerKind == .googleCloud
            && !settings.incognitoMode
        let geminiCommandAllowed = sessionMode == .command
            && settings.cloudFallbackAvailable
            && !settings.incognitoMode
        let cleanupEnabled = settings.faithfulEnhancementEnabled
            && sessionMode != .command
            && (!settings.incognitoMode || providerKind != .googleCloud)
        let contextPolicy = CleanupContextPolicy(
            allowedSources: cloudRewriteAllowed ? [] : Set(sessionContext.sources)
        )
        sessionTranscriptCommander = transcriptCommander
        sessionUsesGeminiCommand = false
        let googleProviders = cloudRewriteAllowed || geminiCommandAllowed
            ? googleCloudRewriteProviderFactory.makeProviders(settings: settings)
            : nil

        let cleanupProvider: any CleanupProvider
        if cloudRewriteAllowed, let googleProviders {
            cleanupProvider = cleanupEnabled
                ? googleProviders.cleanupProvider
                : DisabledCleanupProvider()
            sessionTranscriptStyler = googleProviders.transcriptStyler
            sessionUsesCloudRewrite = true
        } else {
            cleanupProvider = cleanupEnabled
                ? try cleanupProviderRegistry.resolve(
                    id: settings.cleanupProviderID,
                    contextPolicy: contextPolicy,
                    incognito: settings.incognitoMode
                )
                : DisabledCleanupProvider()
            sessionTranscriptStyler = transcriptStyler
            sessionUsesCloudRewrite = false
        }

        if geminiCommandAllowed, let googleProviders {
            sessionTranscriptCommander = PrimaryThenFallbackTranscriptCommander(
                primary: googleProviders.transcriptCommander,
                fallback: transcriptCommander
            )
            sessionUsesGeminiCommand = true
        }

        let cleanupConfiguration = CleanupSessionConfiguration(
            enabled: cleanupEnabled,
            providerID: cleanupProvider.descriptor.id,
            contextPolicy: contextPolicy,
            attemptTimeout: cloudRewriteAllowed ? .seconds(12) : .seconds(8),
            stopDrainTimeout: cloudRewriteAllowed ? .seconds(8) : .seconds(2)
        )
        let cleanupContext = contextPolicy.applying(to: sessionContext)
        sessionSnapshot = RecordingSessionSnapshot(
            id: activeSessionID,
            processingConfiguration: processingConfiguration,
            cleanupConfiguration: cleanupConfiguration,
            cleanupModelIdentifier: cleanupProvider.descriptor.modelIdentifier,
            cleanupContext: cleanupContext,
            incognito: settings.incognitoMode,
            audioRetentionPolicy: settings.audioRetentionPolicy,
            language: settings.language
        )
        activeCleanupProviderName = cleanupEnabled
            ? cleanupProvider.descriptor.displayName
            : nil
        activeCleanupModelIdentifier = cleanupEnabled
            ? cleanupProvider.descriptor.modelIdentifier
            : nil
        activeCleanupContextSources = cleanupEnabled ? cleanupContext.sources : []
        cleanupSession = TranscriptCleanupSession(
            sessionID: activeSessionID,
            configuration: cleanupConfiguration,
            provider: cleanupProvider,
            context: cleanupContext,
            protectedVocabulary: personalizationStore.recognitionContext
        ) { [weak self] projection in
            guard let self, self.sessionID == activeSessionID else { return }
            if self.presentsFeedback {
                FloatingTranscriptWindow.shared.updateCleanupProjection(projection)
            }
        }
    }

    private var commandContextSources: [TranscriptContextSource] {
        mergedContextSources(
            sessionUsesGeminiCommand ? [] : sessionContext.sources,
            sessionTarget?.selectedText == nil ? [] : [.selectedText]
        )
    }

    private var rewriteContextSources: [TranscriptContextSource] {
        switch sessionMode {
        case .dictation:
            let cleanupSources = sessionSnapshot?.cleanupConfiguration.enabled == true
                ? sessionSnapshot?.cleanupContext.sources ?? []
                : []
            let styleSources = sessionStyle != nil && !sessionUsesCloudRewrite
                ? sessionContext.sources
                : []
            return mergedContextSources(cleanupSources, styleSources)
        case .scratchpad:
            return sessionSnapshot?.cleanupConfiguration.enabled == true
                ? sessionSnapshot?.cleanupContext.sources ?? []
                : []
        case .command, .meeting:
            return []
        }
    }

    private var contextSourcesForCurrentMode: [TranscriptContextSource] {
        switch sessionMode {
        case .command:
            commandContextSources
        case .dictation, .scratchpad:
            rewriteContextSources
        case .meeting:
            []
        }
    }

    private func mergedContextSources(
        _ leading: [TranscriptContextSource],
        _ trailing: [TranscriptContextSource]
    ) -> [TranscriptContextSource] {
        var seen: Set<String> = []
        return (leading + trailing).filter { seen.insert($0.rawValue).inserted }
    }

    private func finishCancelledStart() {
        completeStop(transcript: "", completion: { _ in })
    }

    private func notifyStopped() {
        let callback = onStopped
        onStopped = nil
        callback?()
    }

    // MARK: - Filler Removal

    func removeFiller(_ text: String) -> String {
        guard settings.removeFiller else { return text }
        let range = NSRange(text.startIndex..., in: text)
        var result = fillerPattern.stringByReplacingMatches(
            in: text, range: range, withTemplate: ""
        )
        // Collapse multiple spaces
        while result.contains("  ") {
            result = result.replacingOccurrences(of: "  ", with: " ")
        }
        return result.trimmingCharacters(in: .whitespaces)
    }

}
