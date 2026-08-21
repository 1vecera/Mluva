import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Provider-neutral recording controller")
@MainActor
struct ProviderRecordingControllerTests {
    @Test("Audio and recognition flow through the selected provider")
    func routesAudioAndRecognition() async throws {
        let provider = RecordingTranscriptionProvider(finalTranscript: "I um use Mluva")
        let capture = RecordingAudioCapture()
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02]))
        provider.emit(.volatile(id: "provider-segment", text: "I um use Voice"))
        await drainMainQueue()

        #expect(provider.audio == Data([0x01, 0x02]))
        #expect(controller.audioLevel > 0)
        #expect(controller.partialText == "I use Voice")
        #expect(await destination.deliveries.isEmpty)

        let transcript = await stop(controller)

        #expect(transcript == "I use Mluva")
        #expect(controller.lastCommittedText == "I use Mluva")
        #expect(controller.lastSessionResult?.rawText == "I um use Mluva")
        #expect(controller.lastSessionResult?.deliveredText == "I use Mluva")
        #expect(controller.lastSessionResult?.provider == .apple)
        #expect(controller.lastSessionResult?.deliveryOutcome == .delivered)
        #expect(controller.lastSessionResult?.timings.captureLatency != nil)
        #expect(controller.lastSessionResult?.timings.recognitionLatency != nil)
        #expect(controller.lastSessionResult?.timings.enhancementLatency != nil)
        #expect(controller.lastSessionResult?.timings.deliveryLatency != nil)
        #expect(await destination.deliveries == [
            TextDelivery(id: "recording-session", text: "I use Mluva")
        ])
        #expect(controller.state == .idle)
    }

    @Test("Focused application restores its command mode and provider")
    func restoresApplicationProfile() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "make shorter")
        let providerFactory = RecordingProviderFactoryProbe(provider: provider)
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("application-profile-\(UUID().uuidString).json")
        )
        personalization.selectMode(
            .command,
            for: "com.apple.mail",
            rememberPerApplication: true
        )
        personalization.selectProvider(
            .googleCloud,
            for: "com.apple.mail",
            rememberPerApplication: true
        )
        let controller = makeController(
            provider: provider,
            providerFactory: providerFactory,
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            personalization: personalization,
            transcriptCommander: FixedTranscriptCommander(result: "Short"),
            captureTarget: {
                RecordingCommandTarget(
                    selectedText: "This is too long",
                    applicationName: "Mail",
                    bundleIdentifier: "com.apple.mail"
                )
            },
            configureSettings: {
                $0.transcriptionMode = .dictation
                $0.providerPreference = .apple
                $0.rememberLastModePerApplication = true
                $0.rememberProviderPerApplication = true
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(providerFactory.preferences == [.googleCloud])
        #expect(controller.pendingCommandPreview?.proposedText == "Short")
        #expect(controller.lastSessionResult?.mode == .command)
        #expect(controller.settings.transcriptionMode == .dictation)
        #expect(controller.settings.providerPreference == .apple)
    }

    @Test("Provider final callback does not insert before recording stops")
    func providerFinalWaitsForStop() async throws {
        let provider = RecordingTranscriptionProvider(finalTranscript: "ready")
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        provider.emit(.final(id: "provider-segment", text: "ready"))
        await drainMainQueue()

        #expect(await destination.deliveries.isEmpty)

        _ = await stop(controller)
        #expect(await destination.deliveries.count == 1)
    }

    @Test("Repeated stop requests complete once from one delivery")
    func repeatedStopIsIdempotent() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "complete transcript")
        let destination = SlowRecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        let firstStop = Task { await stop(controller) }
        await waitUntil { controller.state == .stopping }
        let secondTranscript = await stop(controller)
        let firstTranscript = await firstStop.value

        #expect(firstTranscript == "complete transcript")
        #expect(secondTranscript == "complete transcript")
        #expect(await destination.deliveries == [
            TextDelivery(id: "recording-session", text: "complete transcript")
        ])
    }

    @Test("A final callback remains available when finish returns no text")
    func finalCallbackIsTheStopFallback() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "")
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        provider.emit(.final(id: "provider-segment", text: "callback transcript"))
        await drainMainQueue()
        let transcript = await stop(controller)

        #expect(transcript == "callback transcript")
        #expect(controller.lastSessionResult?.rawText == "callback transcript")
        #expect(await destination.deliveries == [
            TextDelivery(id: "recording-session", text: "callback transcript")
        ])
    }

    @Test("Provider finish appends only text missing from final callbacks")
    func providerFinishAppendsMissingSuffix() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "hello complete world")
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        provider.emit(.final(id: "provider-segment", text: "hello complete"))
        await drainMainQueue()
        let transcript = await stop(controller)

        #expect(transcript == "hello complete world")
        #expect(controller.lastSessionResult?.rawText == "hello complete world")
        #expect(await destination.deliveries.first?.text == "hello complete world")
    }

    @Test("A final callback survives provider finalization failure")
    func finalCallbackSurvivesFinishFailure() async {
        let provider = RecordingTranscriptionProvider(
            finalTranscript: "",
            finishError: TestFailure.failed
        )
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination,
            configureSettings: { $0.providerPreference = .apple }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        provider.emit(.final(id: "provider-segment", text: "retained final"))
        await drainMainQueue()
        let transcript = await stop(controller)

        #expect(transcript == "retained final")
        #expect(controller.lastSessionResult?.deliveryOutcome == .delivered)
        #expect(await destination.deliveries.first?.text == "retained final")
    }

    @Test("Empty speech finishes without inserting")
    func emptySpeechDoesNotInsert() async {
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "  "),
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        let transcript = await stop(controller)

        #expect(transcript.isEmpty)
        #expect(await destination.deliveries.isEmpty)
    }

    @Test("Provider startup error returns the controller to idle")
    func providerStartupFailureReturnsIdle() async {
        let provider = RecordingTranscriptionProvider(startError: TestFailure.failed)
        let capture = RecordingAudioCapture()
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: RecordingControllerDestination()
        )
        var stopped = false

        controller.start { stopped = true }
        await waitUntil { controller.state == .idle && controller.error != nil }

        #expect(stopped)
        #expect(!capture.started)
        #expect(controller.error == "failed")
    }

    @Test("Repeated stop during startup completes every caller without delivery")
    func repeatedStopDuringStartupIsIdempotent() async {
        let provider = RecordingTranscriptionProvider(
            finalTranscript: "must not deliver",
            startDelay: .milliseconds(50)
        )
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        #expect(controller.state == .starting)
        let firstStop = Task { await stop(controller) }
        let secondStop = Task { await stop(controller) }

        #expect(await firstStop.value.isEmpty)
        #expect(await secondStop.value.isEmpty)
        #expect(controller.state == .idle)
        #expect(provider.cancelled)
        #expect(await destination.deliveries.isEmpty)
    }

    @Test("Meeting mode cannot start through the ordinary dictation controller")
    func meetingRequiresExplicitCapture() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "must not run")
        let capture = RecordingAudioCapture()
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: RecordingControllerDestination(),
            configureSettings: { $0.transcriptionMode = .meeting }
        )
        var stopped = false

        controller.start { stopped = true }
        await waitUntil { controller.state == .idle && controller.error != nil }

        #expect(stopped)
        #expect(!provider.started)
        #expect(!capture.started)
        #expect(controller.error == "Start meetings from the explicit Meeting mode controls.")
    }

    @Test("Automatic provider falls back after startup failure")
    func automaticProviderFallsBackAfterStartupFailure() async {
        let primary = RecordingTranscriptionProvider(
            kind: .apple,
            startError: TestFailure.failed
        )
        let fallback = RecordingTranscriptionProvider(
            kind: .googleCloud,
            finalTranscript: "fallback transcript"
        )
        let factory = FallbackRecordingProviderFactory(
            primary: primary,
            fallback: fallback
        )
        let controller = makeController(
            provider: primary,
            providerFactory: factory,
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            configureSettings: { $0.providerPreference = .automatic }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        primary.emit(.final(id: "stale-primary", text: "must be ignored"))
        await drainMainQueue()
        let transcript = await stop(controller)

        #expect(transcript == "fallback transcript")
        #expect(controller.lastSessionResult?.rawText == "fallback transcript")
        #expect(controller.lastSessionResult?.provider == .googleCloud)
        #expect(controller.lastSessionResult?.fallbackEvent == ProviderFallbackEvent(
            from: .apple,
            to: .googleCloud,
            reason: .providerStartupFailed
        ))
    }

    @Test("Explicit provider choice never falls back")
    func explicitProviderDoesNotFallBack() async {
        let primary = RecordingTranscriptionProvider(
            kind: .apple,
            startError: TestFailure.failed
        )
        let fallback = RecordingTranscriptionProvider(
            kind: .googleCloud,
            finalTranscript: "must not run"
        )
        let controller = makeController(
            provider: primary,
            providerFactory: FallbackRecordingProviderFactory(
                primary: primary,
                fallback: fallback
            ),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            configureSettings: { $0.providerPreference = .apple }
        )

        controller.start {}
        await waitUntil { controller.state == .idle && controller.error != nil }

        #expect(!fallback.started)
        #expect(controller.lastSessionResult == nil)
    }

    @Test("Automatic provider replays audio after finalization failure")
    func automaticProviderFallsBackAfterFinalizationFailure() async {
        let primary = RecordingTranscriptionProvider(
            kind: .googleCloud,
            finishError: TestFailure.failed
        )
        let fallback = RecordingTranscriptionProvider(
            kind: .apple,
            finalTranscript: "recovered transcript"
        )
        let factory = FallbackRecordingProviderFactory(
            primary: primary,
            fallback: fallback
        )
        let capture = RecordingAudioCapture()
        let controller = makeController(
            provider: primary,
            providerFactory: factory,
            capture: capture,
            destination: RecordingControllerDestination(),
            configureSettings: { $0.providerPreference = .automatic }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02, 0x03]))
        let transcript = await stop(controller)

        #expect(transcript == "recovered transcript")
        #expect(fallback.audio == Data([0x01, 0x02, 0x03]))
        #expect(controller.lastSessionResult?.provider == .apple)
        #expect(controller.lastSessionResult?.fallbackEvent == ProviderFallbackEvent(
            from: .googleCloud,
            to: .apple,
            reason: .providerFinalizationFailed
        ))
    }

    @Test("Cancel discards capture without delivering text")
    func cancelDiscardsCapture() async {
        let provider = RecordingTranscriptionProvider(finalTranscript: "do not insert")
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )
        var stopped = false

        controller.start { stopped = true }
        await waitUntil { controller.state == .recording }
        controller.cancel()

        #expect(controller.state == .idle)
        #expect(provider.cancelled)
        #expect(await destination.deliveries.isEmpty)
        #expect(stopped)
    }

    @Test("Dictionary and snippets are applied before final delivery")
    func appliesPersonalization() async {
        let provider = RecordingTranscriptionProvider(
            finalTranscript: "Use post grass snippet email signoff"
        )
        let destination = RecordingControllerDestination()
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("controller-personalization-\(UUID().uuidString).json")
        )
        personalization.saveDictionaryReplacement(spoken: "post grass", written: "Postgres")
        personalization.saveSnippet(trigger: "email signoff", expansion: "Best,\nDaniel")
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination,
            personalization: personalization
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        let transcript = await stop(controller)

        #expect(transcript == "Use Postgres Best,\nDaniel")
        #expect(await destination.deliveries.first?.text == transcript)
    }

    @Test("Opt-in faithful enhancement runs before final delivery")
    func appliesFaithfulEnhancement() async throws {
        let provider = RecordingTranscriptionProvider(finalTranscript: "hello world")
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination,
            transcriptEnhancer: FixedRecordingEnhancer(
                result: TranscriptEnhancementResult(
                    text: "Hello world.",
                    outcome: .applied,
                    violations: []
                )
            ),
            configureSettings: { $0.faithfulEnhancementEnabled = true }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        controller.settings.cleanupProviderID = "missing-after-start"
        let transcript = await stop(controller)

        #expect(transcript == "Hello world.")
        #expect(await destination.deliveries.first?.text == "Hello world.")
        #expect(controller.lastSessionResult?.enhancementOutcome == .applied)
        #expect(controller.lastSessionResult?.cleanupProviderID == "apple-intelligence")
        #expect(
            controller.lastSessionResult?.cleanupModelIdentifier
                == "apple.foundation-model.on-device-v1"
        )
    }

    @Test("Google recognition routes faithful cleanup through Gemini Flash")
    func googleRecognitionUsesGeminiCleanup() async {
        let provider = RecordingTranscriptionProvider(
            kind: .googleCloud,
            finalTranscript: "hello world"
        )
        let destination = RecordingControllerDestination()
        let cloudRewriteFactory = RecordingGoogleCloudRewriteFactory()
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination,
            googleCloudRewriteProviderFactory: cloudRewriteFactory,
            configureSettings: {
                $0.faithfulEnhancementEnabled = true
                $0.googleCloudProjectID = "example-project"
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        let transcript = await stop(controller)

        #expect(transcript == "Hello world.")
        #expect(controller.lastSessionResult?.cleanupProviderID == "test-gemini-flash")
        #expect(controller.lastSessionResult?.cleanupModelIdentifier == "gemini-3.6-flash")
        #expect(controller.lastSessionResult?.contextSources.isEmpty == true)
        #expect(cloudRewriteFactory.makeCount == 1)
        #expect(await destination.deliveries.first?.text == "Hello world.")
    }

    @Test("On-device cleanup receives only consented application context")
    func enhancementReceivesConsentedContext() async throws {
        let enhancer = RecordingTranscriptEnhancer()
        let target = RecordingCommandTarget(
            selectedText: "selected words",
            applicationName: "Notes",
            bundleIdentifier: "com.apple.Notes",
            windowTitle: "Release plan",
            nearbyText: "Nearby document words"
        )
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "ship the release"),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            transcriptEnhancer: enhancer,
            captureTarget: { target },
            configureSettings: {
                $0.faithfulEnhancementEnabled = true
                $0.contextualFormattingEnabled = true
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        let request = try #require(await enhancer.requests.first)
        #expect(request.context.applicationName == "Notes")
        #expect(request.context.windowTitle == "Release plan")
        #expect(request.context.selectedText == "selected words")
        #expect(request.context.nearbyText == "Nearby document words")
        #expect(controller.lastSessionResult?.contextSources == [
            .application,
            .windowTitle,
            .selectedText,
            .nearbyText,
        ])
    }

    @Test("Disabled context never enters on-device cleanup")
    func disabledContextStaysOutOfEnhancement() async throws {
        let enhancer = RecordingTranscriptEnhancer()
        let target = RecordingCommandTarget(
            selectedText: "private selection",
            applicationName: "Mail",
            bundleIdentifier: "com.apple.mail",
            windowTitle: "Private subject",
            nearbyText: "Private message body"
        )
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "hello"),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            transcriptEnhancer: enhancer,
            captureTarget: { target },
            configureSettings: {
                $0.faithfulEnhancementEnabled = true
                $0.contextualFormattingEnabled = true
                $0.setContextualFormatting(false, for: "com.apple.mail")
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        let request = try #require(await enhancer.requests.first)
        #expect(request.context == .empty)
        #expect(controller.lastSessionResult?.contextSources.isEmpty == true)
    }

    @Test("Command mode previews an edit before replacing selected text")
    func commandModeRequiresPreviewAcceptance() async throws {
        let provider = RecordingTranscriptionProvider(
            finalTranscript: "make this friendlier"
        )
        let destination = RecordingControllerDestination()
        let target = RecordingCommandTarget(selectedText: "Send the report today.")
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination,
            transcriptCommander: FixedTranscriptCommander(
                result: "Could you send the report today?"
            ),
            captureTarget: { target },
            configureSettings: { $0.transcriptionMode = .command }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(await destination.deliveries.isEmpty)
        #expect(controller.pendingCommandPreview?.instruction == "make this friendlier")
        #expect(controller.pendingCommandPreview?.sourceText == "Send the report today.")
        #expect(controller.pendingCommandPreview?.proposedText == "Could you send the report today?")
        #expect(controller.lastSessionResult?.mode == .command)
        #expect(controller.lastSessionResult?.deliveryOutcome == .pendingDelivery)
        #expect(controller.lastSessionResult?.contextSources == [.selectedText])

        let acceptedEntry = try await controller.acceptCommandPreview()

        #expect(acceptedEntry?.deliveryOutcome == .delivered)
        #expect(controller.pendingCommandPreview == nil)
        #expect(await destination.deliveries == [
            TextDelivery(id: "recording-session", text: "Could you send the report today?")
        ])
    }

    @Test("Command uses Gemini first even when Apple handled recognition")
    func commandUsesGeminiBeforeLocalFallback() async throws {
        let provider = RecordingTranscriptionProvider(
            kind: .apple,
            finalTranscript: "make this friendlier"
        )
        let rewriteFactory = RecordingGoogleCloudRewriteFactory(
            transcriptCommander: FixedTranscriptCommander(
                result: "Could you send the report today?"
            )
        )
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            googleCloudRewriteProviderFactory: rewriteFactory,
            transcriptCommander: FixedTranscriptCommander(result: "Local command"),
            captureTarget: {
                RecordingCommandTarget(
                    selectedText: "Send the report today.",
                    applicationName: "Private Mail",
                    bundleIdentifier: "com.example.private-mail",
                    windowTitle: "Confidential inbox",
                    nearbyText: "Do not disclose nearby context"
                )
            },
            configureSettings: {
                $0.transcriptionMode = .command
                $0.providerPreference = .apple
                $0.cloudRecognitionAllowed = true
                $0.googleCloudProjectID = "example-project"
                $0.contextualFormattingEnabled = true
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(rewriteFactory.makeCount == 1)
        #expect(controller.pendingCommandPreview?.proposedText == "Could you send the report today?")
        #expect(controller.lastSessionResult?.deliveryOutcome == .pendingDelivery)
        #expect(controller.lastSessionResult?.contextSources == [.selectedText])
    }

    @Test("Command falls back to Apple after an empty Gemini result")
    func commandUsesLocalFallbackAfterGeminiFailure() async throws {
        let rewriteFactory = RecordingGoogleCloudRewriteFactory(
            transcriptCommander: FixedTranscriptCommander(result: "")
        )
        let controller = makeController(
            provider: RecordingTranscriptionProvider(
                kind: .apple,
                finalTranscript: "make this friendlier"
            ),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            googleCloudRewriteProviderFactory: rewriteFactory,
            transcriptCommander: FixedTranscriptCommander(result: "Local command"),
            captureTarget: {
                RecordingCommandTarget(selectedText: "Send the report today.")
            },
            configureSettings: {
                $0.transcriptionMode = .command
                $0.providerPreference = .apple
                $0.cloudRecognitionAllowed = true
                $0.googleCloudProjectID = "example-project"
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(rewriteFactory.makeCount == 1)
        #expect(controller.pendingCommandPreview?.proposedText == "Local command")
    }

    @Test("Incognito command never constructs the Gemini fallback")
    func incognitoCommandStaysLocal() async throws {
        let rewriteFactory = RecordingGoogleCloudRewriteFactory()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(
                kind: .googleCloud,
                finalTranscript: "make this friendlier"
            ),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            googleCloudRewriteProviderFactory: rewriteFactory,
            transcriptCommander: FixedTranscriptCommander(result: "Local command"),
            captureTarget: {
                RecordingCommandTarget(selectedText: "Send the report today.")
            },
            configureSettings: {
                $0.transcriptionMode = .command
                $0.providerPreference = .googleCloud
                $0.cloudRecognitionAllowed = true
                $0.googleCloudProjectID = "example-project"
                $0.incognitoMode = true
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(rewriteFactory.makeCount == 0)
        #expect(controller.pendingCommandPreview?.proposedText == "Local command")
    }

    @Test("Failed recognition retains the captured audio for retry")
    func failedRecognitionRetainsAudio() async throws {
        let provider = RecordingTranscriptionProvider(finishError: TestFailure.failed)
        let capture = RecordingAudioCapture()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("controller-audio-\(UUID().uuidString)")
        let retentionStore = AudioRetentionStore(directoryURL: directory)
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: RecordingControllerDestination(),
            audioRetentionStore: retentionStore
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        let audio = Data([0x01, 0x02, 0x03, 0x04])
        capture.emit(audio)
        _ = await stop(controller)

        let entry = try #require(controller.lastSessionResult)
        let filename = try #require(entry.retainedAudioFilename)
        #expect(entry.deliveryOutcome == .failed)
        #expect(try retentionStore.load(filename: filename) == audio)
    }

    @Test("Failed delivery preserves recognized text for an independent retry")
    func failedDeliveryStaysPending() async throws {
        let provider = RecordingTranscriptionProvider(finalTranscript: "paste this later")
        let destination = RecordingControllerDestination(failuresBeforeSuccess: 1)
        let controller = makeController(
            provider: provider,
            capture: RecordingAudioCapture(),
            destination: destination
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        let entry = try #require(controller.lastSessionResult)
        #expect(entry.rawText == "paste this later")
        #expect(entry.deliveredText == "paste this later")
        #expect(entry.deliveryOutcome == .pendingDelivery)
        #expect(entry.failureMessage == "failed")
        #expect(await destination.deliveries.isEmpty)
    }

    @Test("Incognito capture retains no recovery audio")
    func incognitoDiscardsRecoveryAudio() async throws {
        let provider = RecordingTranscriptionProvider(finishError: TestFailure.failed)
        let capture = RecordingAudioCapture()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("incognito-audio-\(UUID().uuidString)")
        let retentionStore = AudioRetentionStore(directoryURL: directory)
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: RecordingControllerDestination(),
            audioRetentionStore: retentionStore,
            configureSettings: { $0.incognitoMode = true }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02]))
        _ = await stop(controller)

        let entry = try #require(controller.lastSessionResult)
        #expect(entry.retainedAudioFilename == nil)
    }

    @Test("Incognito policy is frozen for the full Scratchpad session")
    func incognitoSnapshotBlocksLateDurableWrites() async throws {
        let draftURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("incognito-scratchpad-\(UUID().uuidString).json")
        let draftStore = ScratchpadDraftStore(fileURL: draftURL)
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "private thought"),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            scratchpadDraftStore: draftStore,
            configureSettings: {
                $0.transcriptionMode = .scratchpad
                $0.incognitoMode = true
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        controller.settings.incognitoMode = false
        _ = await stop(controller)

        #expect(controller.lastSessionWasIncognito)
        #expect(controller.pendingScratchpadDraft?.text == "private thought")
        #expect(!FileManager.default.fileExists(atPath: draftURL.path))
    }

    @Test("Scratchpad capture creates a durable editable draft without inserting")
    func scratchpadCreatesDurableDraft() async throws {
        let provider = RecordingTranscriptionProvider(finalTranscript: "long form thinking")
        let capture = RecordingAudioCapture()
        let destination = RecordingControllerDestination()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("scratchpad-audio-\(UUID().uuidString)")
        let retentionStore = AudioRetentionStore(directoryURL: directory)
        let draftURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("scratchpad-controller-\(UUID().uuidString).json")
        let draftStore = ScratchpadDraftStore(fileURL: draftURL)
        let controller = makeController(
            provider: provider,
            capture: capture,
            destination: destination,
            audioRetentionStore: retentionStore,
            scratchpadDraftStore: draftStore,
            configureSettings: {
                $0.transcriptionMode = .scratchpad
                $0.audioRetentionPolicy = .never
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02, 0x03, 0x04]))
        let transcript = await stop(controller)

        let draft = try #require(controller.pendingScratchpadDraft)
        let filename = try #require(draft.entry.retainedAudioFilename)
        #expect(transcript == "long form thinking")
        #expect(draft.text == "long form thinking")
        #expect(draft.entry.rawText == "long form thinking")
        #expect(draft.entry.mode == .scratchpad)
        #expect(draft.entry.deliveryOutcome == .pendingDelivery)
        #expect(try retentionStore.load(filename: filename) == Data([0x01, 0x02, 0x03, 0x04]))
        #expect(await destination.deliveries.isEmpty)
        #expect(ScratchpadDraftStore(fileURL: draftURL).draft?.id == draft.id)
    }

    @Test("Copying an edited scratchpad accepts it and releases temporary audio")
    func copyAcceptsEditedScratchpad() async throws {
        let capture = RecordingAudioCapture()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("scratchpad-copy-audio-\(UUID().uuidString)")
        let retentionStore = AudioRetentionStore(directoryURL: directory)
        let draftStore = ScratchpadDraftStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("scratchpad-copy-\(UUID().uuidString).json")
        )
        var copiedText: String?
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "initial draft"),
            capture: capture,
            destination: RecordingControllerDestination(),
            audioRetentionStore: retentionStore,
            scratchpadDraftStore: draftStore,
            scratchpadClipboardWriter: {
                copiedText = $0
                return true
            },
            configureSettings: {
                $0.transcriptionMode = .scratchpad
                $0.audioRetentionPolicy = .never
            }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02]))
        _ = await stop(controller)
        let filename = try #require(controller.pendingScratchpadDraft?.entry.retainedAudioFilename)
        controller.updateScratchpadText("edited draft")

        let accepted = try await controller.acceptScratchpadDraft(destination: .clipboard)

        #expect(copiedText == "edited draft")
        #expect(accepted?.deliveredText == "edited draft")
        #expect(accepted?.deliveryOutcome == .delivered)
        #expect(accepted?.retainedAudioFilename == nil)
        #expect(controller.pendingScratchpadDraft == nil)
        #expect(draftStore.draft == nil)
        #expect(!retentionStore.exists(filename: filename))
    }

    @Test("Inserting a scratchpad uses the captured destination only after acceptance")
    func insertAcceptsScratchpad() async throws {
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "initial draft"),
            capture: RecordingAudioCapture(),
            destination: destination,
            scratchpadDraftStore: ScratchpadDraftStore(
                fileURL: FileManager.default.temporaryDirectory
                    .appendingPathComponent("scratchpad-insert-\(UUID().uuidString).json")
            ),
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)
        #expect(await destination.deliveries.isEmpty)
        controller.updateScratchpadText("insert the edited draft")

        let accepted = try await controller.acceptScratchpadDraft(destination: .originalApplication)

        #expect(accepted?.deliveryOutcome == .delivered)
        #expect(await destination.deliveries == [
            TextDelivery(id: "recording-session", text: "insert the edited draft")
        ])
        #expect(controller.pendingScratchpadDraft == nil)
    }

    @Test("Deleting a scratchpad removes its retained audio")
    func discardScratchpadDeletesAudio() async throws {
        let capture = RecordingAudioCapture()
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("scratchpad-discard-audio-\(UUID().uuidString)")
        let retentionStore = AudioRetentionStore(directoryURL: directory)
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "discard me"),
            capture: capture,
            destination: RecordingControllerDestination(),
            audioRetentionStore: retentionStore,
            scratchpadDraftStore: ScratchpadDraftStore(
                fileURL: FileManager.default.temporaryDirectory
                    .appendingPathComponent("scratchpad-discard-\(UUID().uuidString).json")
            ),
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        capture.emit(Data([0x01, 0x02]))
        _ = await stop(controller)
        let filename = try #require(controller.pendingScratchpadDraft?.entry.retainedAudioFilename)

        controller.discardScratchpadDraft()

        #expect(controller.pendingScratchpadDraft == nil)
        #expect(controller.lastSessionResult == nil)
        #expect(!retentionStore.exists(filename: filename))
    }

    @Test("Failed scratchpad copy remains recoverable and reports the failure")
    func failedScratchpadCopyRemainsPending() async throws {
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "keep this draft"),
            capture: RecordingAudioCapture(),
            destination: RecordingControllerDestination(),
            scratchpadClipboardWriter: { _ in false },
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        await #expect(throws: ScratchpadDraftError.clipboardUnavailable) {
            try await controller.acceptScratchpadDraft(destination: .clipboard)
        }
        #expect(controller.pendingScratchpadDraft?.text == "keep this draft")
        #expect(controller.error == ScratchpadDraftError.clipboardUnavailable.localizedDescription)
    }

    @Test("Simultaneous scratchpad acceptance delivers exactly once")
    func scratchpadAcceptanceIsSerialized() async throws {
        let destination = SlowRecordingControllerDestination()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "deliver once"),
            capture: RecordingAudioCapture(),
            destination: destination,
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        async let first = controller.acceptScratchpadDraft(destination: .originalApplication)
        async let second = controller.acceptScratchpadDraft(destination: .originalApplication)
        let accepted = try await [first, second]

        #expect(accepted.compactMap { $0 }.count == 1)
        #expect(await destination.deliveries.count == 1)
    }

    @Test("Selected style transforms dictation before final delivery")
    func selectedStyleProcessesDictation() async throws {
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("dictation-style-\(UUID().uuidString).json")
        )
        let style = try #require(personalization.styles.first { $0.name == "Message" })
        personalization.selectStyle(
            style.id,
            for: nil,
            rememberPerApplication: false
        )
        let styler = RecordingTranscriptStyler(
            text: "Hello team — the release is ready."
        )
        let destination = RecordingControllerDestination()
        let target = RecordingCommandTarget(
            selectedText: nil,
            applicationName: "Messages",
            bundleIdentifier: "com.apple.MobileSMS",
            windowTitle: "Release team",
            nearbyText: "Previous team messages"
        )
        let controller = makeController(
            provider: RecordingTranscriptionProvider(
                finalTranscript: "hello team the release is ready"
            ),
            capture: RecordingAudioCapture(),
            destination: destination,
            personalization: personalization,
            transcriptStyler: styler,
            captureTarget: { target },
            configureSettings: { $0.contextualFormattingEnabled = true }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)

        #expect(await destination.deliveries.map(\.text) == [
            "Hello team — the release is ready."
        ])
        #expect(controller.lastSessionResult?.styleName == "Message")
        #expect(controller.lastSessionResult?.styleOutcome == .applied)
        let request = try #require(await styler.requests.first)
        #expect(request.context.applicationName == "Messages")
        #expect(request.context.windowTitle == "Release team")
        #expect(controller.lastSessionResult?.contextSources == [
            .application,
            .windowTitle,
            .nearbyText,
        ])
    }

    @Test("Scratchpad style processing updates only the reviewable draft")
    func scratchpadStyleRemainsPreviewOnly() async throws {
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("scratchpad-style-\(UUID().uuidString).json")
        )
        let style = try #require(
            personalization.styles.first { $0.name == "Technical notes" }
        )
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(
                finalTranscript: "deploy postgres seventeen with dry run"
            ),
            capture: RecordingAudioCapture(),
            destination: destination,
            personalization: personalization,
            transcriptStyler: FixedTranscriptStyler(
                result: TranscriptStyleResult(
                    text: "Deploy Postgres 17\n- Use dry run",
                    outcome: .applied,
                    violations: []
                )
            ),
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)
        controller.updateScratchpadStyle(style.id)

        let result = try await controller.applyStyleToScratchpadDraft()

        #expect(result?.text == "Deploy Postgres 17\n- Use dry run")
        #expect(result?.appliedStyleName == "Technical notes")
        #expect(controller.pendingScratchpadDraft?.entry.rawText
            == "deploy postgres seventeen with dry run")
        #expect(await destination.deliveries.isEmpty)
    }

    @Test("Scratchpad cannot deliver while a style rewrite is still running")
    func scratchpadDeliveryWaitsForStyle() async throws {
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("scratchpad-style-lock-\(UUID().uuidString).json")
        )
        let style = try #require(
            personalization.styles.first { $0.name == "Message" }
        )
        let destination = RecordingControllerDestination()
        let controller = makeController(
            provider: RecordingTranscriptionProvider(finalTranscript: "a draft"),
            capture: RecordingAudioCapture(),
            destination: destination,
            personalization: personalization,
            transcriptStyler: SlowTranscriptStyler(),
            configureSettings: { $0.transcriptionMode = .scratchpad }
        )

        controller.start {}
        await waitUntil { controller.state == .recording }
        _ = await stop(controller)
        controller.updateScratchpadStyle(style.id)

        async let styled = controller.applyStyleToScratchpadDraft()
        await waitUntil { controller.isScratchpadStyleProcessingInProgress }
        let accepted = try await controller.acceptScratchpadDraft(
            destination: .originalApplication
        )
        let result = try await styled

        #expect(accepted == nil)
        #expect(result?.text == "Styled a draft")
        #expect(controller.pendingScratchpadDraft != nil)
        #expect(await destination.deliveries.isEmpty)
    }

    private func makeController(
        provider: RecordingTranscriptionProvider,
        providerFactory: (any TranscriptionProviderBuilding)? = nil,
        capture: RecordingAudioCapture,
        destination: any TextDestination,
        personalization: PersonalizationStore? = nil,
        audioRetentionStore: AudioRetentionStore = .shared,
        scratchpadDraftStore: ScratchpadDraftStore = ScratchpadDraftStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("scratchpad-test-\(UUID().uuidString).json")
        ),
        transcriptEnhancer: any TranscriptEnhancing = FixedRecordingEnhancer(),
        googleCloudRewriteProviderFactory: any GoogleCloudRewriteProviderBuilding = DefaultGoogleCloudRewriteProviderFactory(),
        transcriptCommander: any TranscriptCommanding = FixedTranscriptCommander(),
        transcriptStyler: any TranscriptStyling = FixedTranscriptStyler(),
        scratchpadClipboardWriter: @escaping (String) -> Bool = { _ in true },
        captureTarget: @escaping () -> (any TextTargetRestoring)? = { nil },
        configureSettings: (AppSettings) -> Void = { _ in }
    ) -> RecordingController {
        let settings = AppSettings(
            defaults: UserDefaults(suiteName: "provider-controller-\(UUID().uuidString)")!
        )
        settings.removeFiller = true
        configureSettings(settings)
        return RecordingController(
            settings: settings,
            providerFactory: providerFactory
                ?? FixedTranscriptionProviderFactory(provider: provider),
            audioCaptureFactory: { capture },
            destination: destination,
            makeSessionID: { "recording-session" },
            presentsFeedback: false,
            personalizationStore: personalization ?? PersonalizationStore(
                fileURL: FileManager.default.temporaryDirectory
                    .appendingPathComponent("empty-personalization-\(UUID().uuidString).json")
            ),
            audioRetentionStore: audioRetentionStore,
            scratchpadDraftStore: scratchpadDraftStore,
            transcriptEnhancer: transcriptEnhancer,
            googleCloudRewriteProviderFactory: googleCloudRewriteProviderFactory,
            transcriptCommander: transcriptCommander,
            transcriptStyler: transcriptStyler,
            scratchpadClipboardWriter: scratchpadClipboardWriter,
            captureTarget: captureTarget
        )
    }

    private func stop(_ controller: RecordingController) async -> String {
        await withCheckedContinuation { continuation in
            controller.stop { continuation.resume(returning: $0) }
        }
    }

    private func waitUntil(
        _ condition: @escaping () -> Bool,
        timeout: Duration = .seconds(10)
    ) async {
        let clock = ContinuousClock()
        let deadline = clock.now.advanced(by: timeout)
        while !condition(), clock.now < deadline {
            try? await Task.sleep(for: .milliseconds(10))
        }
        if !condition() {
            Issue.record("Timed out waiting for the recording controller state to change")
        }
    }

    private func drainMainQueue() async {
        await withCheckedContinuation { continuation in
            DispatchQueue.main.async { continuation.resume() }
        }
    }
}

private final class RecordingGoogleCloudRewriteFactory: GoogleCloudRewriteProviderBuilding, @unchecked Sendable {
    private let lock = NSLock()
    private let transcriptCommander: any TranscriptCommanding
    private var storedMakeCount = 0

    init(
        transcriptCommander: any TranscriptCommanding = FixedTranscriptCommander(
            result: "Cloud command"
        )
    ) {
        self.transcriptCommander = transcriptCommander
    }

    var makeCount: Int {
        lock.lock()
        defer { lock.unlock() }
        return storedMakeCount
    }

    func makeProviders(settings: AppSettings) -> GoogleCloudRewriteProviders {
        lock.lock()
        storedMakeCount += 1
        lock.unlock()
        return GoogleCloudRewriteProviders(
            cleanupProvider: RecordingGeminiCleanupProvider(),
            transcriptStyler: FixedTranscriptStyler(),
            transcriptCommander: transcriptCommander
        )
    }
}

private struct RecordingGeminiCleanupProvider: CleanupProvider {
    let descriptor = CleanupProviderDescriptor(
        id: "test-gemini-flash",
        displayName: "Gemini Flash",
        modelIdentifier: "gemini-3.6-flash",
        capabilities: .cloudTranscriptOnly
    )

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        .success("Hello world.")
    }
}

private struct FixedTranscriptCommander: TranscriptCommanding {
    let result: String

    init(result: String = "") {
        self.result = result
    }

    func execute(_ request: TranscriptCommandRequest) async throws -> String {
        result
    }
}

private final class RecordingCommandTarget: TextTargetRestoring {
    let selectedText: String?
    let targetApplicationName: String?
    let targetBundleIdentifier: String?
    let windowTitle: String?
    let nearbyText: String?

    init(
        selectedText: String?,
        applicationName: String? = nil,
        bundleIdentifier: String? = nil,
        windowTitle: String? = nil,
        nearbyText: String? = nil
    ) {
        self.selectedText = selectedText
        targetApplicationName = applicationName
        targetBundleIdentifier = bundleIdentifier
        self.windowTitle = windowTitle
        self.nearbyText = nearbyText
    }

    func restore() -> Bool { true }
}

private actor RecordingTranscriptEnhancer: TranscriptEnhancing {
    private(set) var requests: [TranscriptEnhancementRequest] = []

    func enhance(_ request: TranscriptEnhancementRequest) async -> TranscriptEnhancementResult {
        requests.append(request)
        return TranscriptEnhancementResult(
            text: request.text,
            outcome: .applied,
            violations: []
        )
    }
}

private struct FixedRecordingEnhancer: TranscriptEnhancing {
    let result: TranscriptEnhancementResult

    init(
        result: TranscriptEnhancementResult = TranscriptEnhancementResult(
            text: "",
            outcome: .unavailable,
            violations: []
        )
    ) {
        self.result = result
    }

    func enhance(_ request: TranscriptEnhancementRequest) async -> TranscriptEnhancementResult {
        result.text.isEmpty
            ? TranscriptEnhancementResult(
                text: request.text,
                outcome: .unavailable,
                violations: []
            )
            : result
    }
}

private struct FixedTranscriptStyler: TranscriptStyling {
    let result: TranscriptStyleResult?

    init(result: TranscriptStyleResult? = nil) {
        self.result = result
    }

    func apply(_ request: TranscriptStyleRequest) async -> TranscriptStyleResult {
        result ?? TranscriptStyleResult(
            text: request.text,
            outcome: .unavailable,
            violations: []
        )
    }
}

private struct SlowTranscriptStyler: TranscriptStyling {
    func apply(_ request: TranscriptStyleRequest) async -> TranscriptStyleResult {
        try? await Task.sleep(for: .milliseconds(100))
        return TranscriptStyleResult(
            text: "Styled \(request.text)",
            outcome: .applied,
            violations: []
        )
    }
}

private actor RecordingTranscriptStyler: TranscriptStyling {
    private(set) var requests: [TranscriptStyleRequest] = []
    private let text: String

    init(text: String) {
        self.text = text
    }

    func apply(_ request: TranscriptStyleRequest) async -> TranscriptStyleResult {
        requests.append(request)
        return TranscriptStyleResult(
            text: text,
            outcome: .applied,
            violations: []
        )
    }
}

private enum TestFailure: Error, LocalizedError {
    case failed

    var errorDescription: String? { "failed" }
}

private struct FixedTranscriptionProviderFactory: TranscriptionProviderBuilding {
    let provider: RecordingTranscriptionProvider

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        provider
    }
}

private final class RecordingProviderFactoryProbe: TranscriptionProviderBuilding {
    let provider: RecordingTranscriptionProvider
    private(set) var preferences: [TranscriptionProviderKind?] = []

    init(provider: RecordingTranscriptionProvider) {
        self.provider = provider
    }

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        provider
    }

    func makeProvider(
        settings: AppSettings,
        preference: TranscriptionProviderKind?
    ) throws -> any TranscriptionProvider {
        preferences.append(preference)
        return provider
    }
}

private final class FallbackRecordingProviderFactory: TranscriptionProviderBuilding {
    let primary: RecordingTranscriptionProvider
    let fallback: RecordingTranscriptionProvider

    init(
        primary: RecordingTranscriptionProvider,
        fallback: RecordingTranscriptionProvider
    ) {
        self.primary = primary
        self.fallback = fallback
    }

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        primary
    }

    func makeProvider(
        settings: AppSettings,
        preference: TranscriptionProviderKind?
    ) throws -> any TranscriptionProvider {
        primary
    }

    func makeFallbackProvider(
        settings: AppSettings,
        excluding provider: TranscriptionProviderKind
    ) throws -> (any TranscriptionProvider)? {
        fallback
    }
}

private final class RecordingTranscriptionProvider: TranscriptionProvider {
    let kind: TranscriptionProviderKind
    var onEvent: ((TranscriptEvent) -> Void)?
    private(set) var audio = Data()
    private(set) var started = false
    private(set) var cancelled = false

    private let finalTranscript: String
    private let startDelay: Duration
    private let startError: Error?
    private let finishError: Error?

    init(
        kind: TranscriptionProviderKind = .apple,
        finalTranscript: String = "",
        startDelay: Duration = .zero,
        startError: Error? = nil,
        finishError: Error? = nil
    ) {
        self.kind = kind
        self.finalTranscript = finalTranscript
        self.startDelay = startDelay
        self.startError = startError
        self.finishError = finishError
    }

    func start() async throws {
        if startDelay > .zero {
            try? await Task.sleep(for: startDelay)
        }
        if let startError { throw startError }
        started = true
    }

    func appendAudio(_ data: Data) {
        audio.append(data)
    }

    func finish() async throws -> String {
        if let finishError { throw finishError }
        return finalTranscript
    }

    func cancel() {
        cancelled = true
    }

    func emit(_ event: TranscriptEvent) {
        onEvent?(event)
    }
}

private final class RecordingAudioCapture: AudioCapturing {
    var onAudioChunk: ((Data) -> Void)?
    private(set) var started = false

    func start() throws {
        started = true
    }

    func stop() {
        started = false
    }

    func emit(_ data: Data) {
        onAudioChunk?(data)
    }
}

private actor RecordingControllerDestination: TextDestination {
    private var remainingFailures: Int
    private(set) var deliveries: [TextDelivery] = []

    init(failuresBeforeSuccess: Int = 0) {
        remainingFailures = failuresBeforeSuccess
    }

    func insert(_ delivery: TextDelivery) async throws {
        if remainingFailures > 0 {
            remainingFailures -= 1
            throw TestFailure.failed
        }
        deliveries.append(delivery)
    }
}

private actor SlowRecordingControllerDestination: TextDestination {
    private(set) var deliveries: [TextDelivery] = []

    func insert(_ delivery: TextDelivery) async throws {
        try await Task.sleep(for: .milliseconds(100))
        deliveries.append(delivery)
    }
}
