import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Provider-neutral transcript cleanup")
struct TranscriptCleanupSessionTests {
    @Test("Registry rejects duplicate provider IDs")
    func duplicateProviderIDsFail() {
        let provider = FixedCleanupProvider()

        #expect(throws: CleanupProviderRegistryError.duplicateProviderID("fake-cleanup")) {
            _ = try CleanupProviderRegistry(providers: [provider, provider])
        }
    }

    @Test("Registry rejects unknown providers and unstable model aliases")
    func providerIdentityValidation() throws {
        let registry = try CleanupProviderRegistry(providers: [FixedCleanupProvider()])
        let policy = CleanupContextPolicy(allowedSources: [])

        #expect(throws: CleanupProviderRegistryError.unknownProviderID("missing")) {
            _ = try registry.resolve(id: "missing", contextPolicy: policy, incognito: false)
        }
        #expect(throws: CleanupProviderRegistryError.unstableModelIdentifier("gemini-latest")) {
            _ = try CleanupProviderRegistry(providers: [
                FixedCleanupProvider(modelIdentifier: "gemini-latest")
            ])
        }
        #expect(throws: CleanupProviderRegistryError.unstableModelIdentifier("claude-sonnet")) {
            _ = try CleanupProviderRegistry(providers: [
                FixedCleanupProvider(modelIdentifier: "claude-sonnet")
            ])
        }
    }

    @Test("Registry rejects context and Incognito capability overreach")
    func capabilityValidation() throws {
        let contextLimited = FixedCleanupProvider(
            capabilities: CleanupProviderCapabilities(
                supportedContextSources: [.application],
                supportsIncognito: true,
                hasDurableSideEffects: false
            )
        )
        let registry = try CleanupProviderRegistry(providers: [contextLimited])
        let selectedTextPolicy = CleanupContextPolicy(allowedSources: [.selectedText])

        #expect(throws: CleanupProviderRegistryError.unsupportedContext("fake-cleanup")) {
            _ = try registry.resolve(
                id: "fake-cleanup",
                contextPolicy: selectedTextPolicy,
                incognito: false
            )
        }

        let durableProvider = FixedCleanupProvider(
            id: "durable-cleanup",
            capabilities: CleanupProviderCapabilities(
                supportedContextSources: [],
                supportsIncognito: true,
                hasDurableSideEffects: true
            )
        )
        let durableRegistry = try CleanupProviderRegistry(providers: [durableProvider])
        #expect(throws: CleanupProviderRegistryError.incognitoDurabilityConflict("durable-cleanup")) {
            _ = try durableRegistry.resolve(
                id: "durable-cleanup",
                contextPolicy: CleanupContextPolicy(allowedSources: []),
                incognito: true
            )
        }
    }

    @Test("Context allowlist enforces per-source and total bounds")
    func contextBounds() {
        let policy = CleanupContextPolicy(
            allowedSources: [.application, .selectedText],
            perSourceCharacterLimit: 5,
            totalCharacterLimit: 8,
            responseCharacterLimit: 20
        )
        let context = policy.applying(to: TranscriptContext(
            applicationName: "abcdef",
            windowTitle: "secret window",
            selectedText: "uvwxyz",
            nearbyText: "secret nearby"
        ))

        #expect(context.applicationName == "abcde")
        #expect(context.windowTitle == nil)
        #expect(context.selectedText == "uvw")
        #expect(context.nearbyText == nil)
        #expect(context.sources == [.application, .selectedText])
    }

    @Test("Outbound cleanup requests contain only disclosed bounded fields")
    @MainActor
    func outboundRequestIsBounded() async throws {
        let responses = ControlledCleanupResponses()
        let provider = ControlledCleanupProvider(responses: responses)
        let policy = CleanupContextPolicy(
            allowedSources: [.application, .selectedText],
            perSourceCharacterLimit: 4,
            totalCharacterLimit: 5,
            segmentCharacterLimit: 6,
            vocabularyItemLimit: 1,
            vocabularyItemCharacterLimit: 3,
            responseCharacterLimit: 12
        )
        let session = TranscriptCleanupSession(
            sessionID: "bounded-session",
            configuration: CleanupSessionConfiguration(
                enabled: true,
                providerID: provider.descriptor.id,
                contextPolicy: policy
            ),
            provider: provider,
            context: TranscriptContext(
                applicationName: "Editor",
                windowTitle: "undisclosed",
                selectedText: "selection",
                nearbyText: "undisclosed"
            ),
            protectedVocabulary: ["Mluva", "Postgres"],
            onProjection: { _ in }
        )

        session.acceptStableSegment(
            id: "bounded",
            rawText: "raw transcript",
            preparedText: "prepared transcript"
        )
        await waitUntil { await responses.requestCount == 1 }
        let request = try #require(await responses.capturedRequests.first)

        #expect(request.rawText == "raw tr")
        #expect(request.preparedText == "prepar")
        #expect(request.context.applicationName == "Edit")
        #expect(request.context.selectedText == "s")
        #expect(request.context.windowTitle == nil)
        #expect(request.context.nearbyText == nil)
        #expect(request.protectedVocabulary == ["Voi"])
        #expect(request.maximumResponseCharacters == 12)

        session.cancel()
    }

    @Test("Concurrent cleanup publishes completed revisions in capture order")
    @MainActor
    func orderedConcurrentPublication() async {
        let responses = ControlledCleanupResponses()
        let provider = ControlledCleanupProvider(responses: responses)
        var projections: [CleanupSessionProjection] = []
        let session = makeSession(provider: provider) { projections.append($0) }

        session.acceptStableSegment(id: "alpha", rawText: "raw alpha", preparedText: "alpha")
        session.acceptStableSegment(id: "beta", rawText: "raw beta", preparedText: "beta")
        await waitUntil { await responses.requestCount == 2 }

        await responses.complete(id: "beta", result: .success("clean beta"))
        await waitUntil {
            projections.last?.segments.last?.state == .waiting
        }
        #expect(projections.last?.segments.map(\.state) == [.rewriting, .waiting])

        let stopTask = Task { await session.stopAndDrain() }
        await responses.complete(id: "alpha", result: .success("clean alpha"))
        let snapshot = await stopTask.value

        #expect(snapshot.segments.map(\.id) == ["alpha", "beta"])
        #expect(snapshot.selectedText == "clean alpha clean beta")
        #expect(projections.last?.segments.map(\.state) == [.cleaned, .cleaned])
    }

    @Test("Provider failure and oversized output publish exact Raw Text")
    @MainActor
    func rawFallbackIsImmutable() async {
        let failureProvider = FixedCleanupProvider(result: .failure(.quota))
        let failureSession = makeSession(provider: failureProvider)
        failureSession.acceptStableSegment(
            id: "failure",
            rawText: "I um said raw",
            preparedText: "I said prepared"
        )
        let failed = await failureSession.stopAndDrain()

        #expect(failed.rawText == "I um said raw")
        #expect(failed.selectedText == "I um said raw")
        #expect(failed.segments.first?.failure == .quota)

        let oversizedProvider = FixedCleanupProvider(result: .success(String(repeating: "x", count: 40)))
        let oversizedSession = makeSession(
            provider: oversizedProvider,
            responseCharacterLimit: 20
        )
        oversizedSession.acceptStableSegment(
            id: "oversized",
            rawText: "raw survives",
            preparedText: "prepared"
        )
        let oversized = await oversizedSession.stopAndDrain()

        #expect(oversized.selectedText == "raw survives")
        #expect(oversized.segments.first?.failure == .outputTooLarge)
    }

    @Test("Meaning-changing cleanup falls back before publication")
    @MainActor
    func unsafeCleanupFallsBackToRaw() async {
        let session = makeSession(
            provider: FixedCleanupProvider(result: .success("Deploy now"))
        )
        session.acceptStableSegment(
            id: "unsafe",
            rawText: "Deploy https://example.com now",
            preparedText: "Deploy https://example.com now"
        )

        let snapshot = await session.stopAndDrain()

        #expect(snapshot.selectedText == "Deploy https://example.com now")
        #expect(snapshot.segments.first?.failure == .safety)
    }

    @Test("Raw fallback removes provider rollover overlap")
    @MainActor
    func rawFallbackDeduplicatesRollover() async {
        let session = makeSession(
            provider: FixedCleanupProvider(result: .failure(.provider))
        )
        session.acceptStableSegment(
            id: "rollover-a",
            rawText: "deploy Mluva to production",
            preparedText: "deploy Mluva to production"
        )
        session.acceptStableSegment(
            id: "rollover-b",
            rawText: "to production after checks pass",
            preparedText: "to production after checks pass"
        )

        let snapshot = await session.stopAndDrain()

        #expect(snapshot.rawText == "deploy Mluva to production after checks pass")
        #expect(snapshot.selectedText == snapshot.rawText)
    }

    @Test("Capacity overflow never blocks speech and releases with Raw Text")
    @MainActor
    func capacityFallback() async {
        let responses = ControlledCleanupResponses()
        let provider = ControlledCleanupProvider(responses: responses)
        let session = makeSession(
            provider: provider,
            concurrencyLimit: 1,
            pendingCapacity: 0
        )
        session.acceptStableSegment(id: "slow", rawText: "raw slow", preparedText: "slow")
        session.acceptStableSegment(id: "overflow", rawText: "raw overflow", preparedText: "overflow")
        await waitUntil { await responses.requestCount == 1 }

        let stopTask = Task { await session.stopAndDrain() }
        await responses.complete(id: "slow", result: .success("clean slow"))
        let snapshot = await stopTask.value

        #expect(snapshot.selectedText == "clean slow raw overflow")
        #expect(snapshot.segments.last?.failure == .skippedCapacity)
    }

    @Test("Timeout settles within the bound and ignores a late success")
    @MainActor
    func timeoutRejectsLateResult() async {
        var projections: [CleanupSessionProjection] = []
        let session = makeSession(
            provider: SlowCancellationIgnoringProvider(),
            attemptTimeout: .milliseconds(10),
            stopDrainTimeout: .milliseconds(100)
        ) { projections.append($0) }
        session.acceptStableSegment(
            id: "timeout",
            rawText: "raw timeout",
            preparedText: "prepared timeout"
        )

        let snapshot = await session.stopAndDrain()
        #expect(snapshot.selectedText == "raw timeout")
        #expect(snapshot.segments.first?.failure == .timeout)

        try? await Task.sleep(for: .milliseconds(20))
        #expect(projections.last?.segments.first?.state == .fallback)
        #expect(projections.filter {
            $0.segments.first?.state == .cleaned
        }.count == 0)
    }

    @Test("Cancellation rejects duplicate and late callbacks")
    @MainActor
    func cancellationAndDuplicateEvents() async {
        let responses = ControlledCleanupResponses()
        let provider = ControlledCleanupProvider(responses: responses)
        var projections: [CleanupSessionProjection] = []
        let session = makeSession(provider: provider) { projections.append($0) }
        session.acceptStableSegment(id: "same", rawText: "raw", preparedText: "prepared")
        session.acceptStableSegment(id: "same", rawText: "mutated", preparedText: "mutated")
        await waitUntil { await responses.requestCount == 1 }

        session.cancel()
        await responses.complete(id: "same", result: .success("late"))
        try? await Task.sleep(for: .milliseconds(10))

        #expect(projections.last?.segments.count == 1)
        #expect(projections.last?.segments.first?.rawText == "raw")
        #expect(projections.last?.segments.first?.state == .cancelled)
    }

    @MainActor
    private func makeSession(
        provider: any CleanupProvider,
        responseCharacterLimit: Int = 8_000,
        concurrencyLimit: Int = 2,
        pendingCapacity: Int = 8,
        attemptTimeout: Duration = .seconds(1),
        stopDrainTimeout: Duration = .seconds(1),
        onProjection: @escaping (CleanupSessionProjection) -> Void = { _ in }
    ) -> TranscriptCleanupSession {
        TranscriptCleanupSession(
            sessionID: "cleanup-session",
            configuration: CleanupSessionConfiguration(
                enabled: true,
                providerID: provider.descriptor.id,
                contextPolicy: CleanupContextPolicy(
                    allowedSources: [.application, .selectedText],
                    responseCharacterLimit: responseCharacterLimit
                ),
                concurrencyLimit: concurrencyLimit,
                pendingCapacity: pendingCapacity,
                attemptTimeout: attemptTimeout,
                stopDrainTimeout: stopDrainTimeout
            ),
            provider: provider,
            context: TranscriptContext(
                applicationName: "Editor",
                selectedText: "selection",
                nearbyText: "must not leave"
            ),
            protectedVocabulary: ["Mluva"],
            onProjection: onProjection
        )
    }

    private func waitUntil(
        _ predicate: @escaping () async -> Bool
    ) async {
        for _ in 0..<200 {
            if await predicate() { return }
            await Task.yield()
        }
        Issue.record("Timed out waiting for cleanup test state")
    }
}

private struct FixedCleanupProvider: CleanupProvider {
    let descriptor: CleanupProviderDescriptor
    let result: CleanupProviderResult

    init(
        id: String = "fake-cleanup",
        modelIdentifier: String = "fake.model-v1",
        capabilities: CleanupProviderCapabilities = .localEphemeral,
        result: CleanupProviderResult = .success("cleaned")
    ) {
        descriptor = CleanupProviderDescriptor(
            id: id,
            displayName: "Fake Cleanup",
            modelIdentifier: modelIdentifier,
            capabilities: capabilities
        )
        self.result = result
    }

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        result
    }
}

private actor ControlledCleanupResponses {
    private var requests: [CleanupRequest] = []
    private var continuations: [String: CheckedContinuation<CleanupProviderResult, Never>] = [:]
    private var buffered: [String: CleanupProviderResult] = [:]

    var requestCount: Int { requests.count }
    var capturedRequests: [CleanupRequest] { requests }

    func result(for request: CleanupRequest) async -> CleanupProviderResult {
        requests.append(request)
        if let result = buffered.removeValue(forKey: request.segmentID) {
            return result
        }
        return await withCheckedContinuation {
            continuations[request.segmentID] = $0
        }
    }

    func complete(id: String, result: CleanupProviderResult) {
        if let continuation = continuations.removeValue(forKey: id) {
            continuation.resume(returning: result)
        } else {
            buffered[id] = result
        }
    }
}

private struct ControlledCleanupProvider: CleanupProvider {
    let descriptor = CleanupProviderDescriptor(
        id: "controlled-cleanup",
        displayName: "Controlled Cleanup",
        modelIdentifier: "controlled.model-v1",
        capabilities: .localEphemeral
    )
    let responses: ControlledCleanupResponses

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        await responses.result(for: request)
    }
}

private struct SlowCancellationIgnoringProvider: CleanupProvider {
    let descriptor = CleanupProviderDescriptor(
        id: "slow-cleanup",
        displayName: "Slow Cleanup",
        modelIdentifier: "slow.model-v1",
        capabilities: .localEphemeral
    )

    func cleanup(_ request: CleanupRequest) async -> CleanupProviderResult {
        try? await Task.sleep(for: .seconds(1))
        return .success("late success")
    }
}
