import Foundation

enum CleanupSegmentProjectionState: String, Equatable, Sendable {
    case raw
    case rewriting
    case waiting
    case cleaned
    case fallback
    case cancelled
}

struct CleanupSegmentProjection: Equatable, Identifiable, Sendable {
    let id: String
    let sequence: Int
    let rawText: String
    let revisedText: String?
    let state: CleanupSegmentProjectionState
    let failure: CleanupProviderFailureCategory?
}

struct CleanupSessionProjection: Equatable, Sendable {
    let cleanupProviderName: String
    let cleanupModelIdentifier: String
    let disclosedContextSources: [TranscriptContextSource]
    let volatileText: String?
    let segments: [CleanupSegmentProjection]
}

struct CleanupSessionConfiguration: Equatable, Sendable {
    let enabled: Bool
    let providerID: String
    let voiceProfile: VoiceProfileSnapshot
    let contextPolicy: CleanupContextPolicy
    let concurrencyLimit: Int
    let pendingCapacity: Int
    let attemptTimeout: Duration
    let stopDrainTimeout: Duration

    init(
        enabled: Bool,
        providerID: String,
        voiceProfile: VoiceProfileSnapshot = .faithful,
        contextPolicy: CleanupContextPolicy,
        concurrencyLimit: Int = 2,
        pendingCapacity: Int = 8,
        attemptTimeout: Duration = .seconds(8),
        stopDrainTimeout: Duration = .seconds(2)
    ) {
        self.enabled = enabled
        self.providerID = providerID
        self.voiceProfile = voiceProfile
        self.contextPolicy = contextPolicy
        self.concurrencyLimit = max(1, concurrencyLimit)
        self.pendingCapacity = max(0, pendingCapacity)
        self.attemptTimeout = attemptTimeout
        self.stopDrainTimeout = stopDrainTimeout
    }
}

struct CleanupTerminalSegment: Equatable, Sendable {
    let id: String
    let sequence: Int
    let rawText: String
    let selectedText: String
    let failure: CleanupProviderFailureCategory?
}

struct CleanupTerminalSnapshot: Equatable, Sendable {
    let sessionID: String
    let providerID: String
    let modelIdentifier: String
    let segments: [CleanupTerminalSegment]

    var rawText: String {
        TranscriptAccumulator.mergeFinalSegments(segments.map(\.rawText))
    }

    var selectedText: String {
        TranscriptAccumulator.mergeFinalSegments(segments.map(\.selectedText))
    }

    var enhancementOutcome: TranscriptEnhancementOutcome {
        if segments.contains(where: { $0.failure == .safety }) {
            return .rejectedUnsafe
        }
        if segments.contains(where: { $0.failure != nil }) {
            return .unavailable
        }
        return segments.contains(where: { $0.selectedText != $0.rawText })
            ? .applied
            : .notRequested
    }

}

@MainActor
final class TranscriptCleanupSession {
    private enum Lifecycle {
        case active
        case stopping
        case cancelled
        case finished
    }

    private enum OperationalState {
        case raw
        case queued
        case running(UUID)
        case waiting(CleanupProviderResult)
        case published(CleanupProviderResult?)
    }

    private struct SegmentRecord {
        let id: String
        let sequence: Int
        let rawText: String
        let preparedText: String
        var state: OperationalState
    }

    private struct AttemptTasks {
        let attemptID: UUID
        let providerTask: Task<Void, Never>
        let timeoutTask: Task<Void, Never>
    }

    let sessionID: String
    let configuration: CleanupSessionConfiguration
    let providerDescriptor: CleanupProviderDescriptor

    var hasStableSegments: Bool { !segments.isEmpty }

    private let provider: any CleanupProvider
    private let context: TranscriptContext
    private let protectedVocabulary: [String]
    private let integrityValidator = TranscriptIntegrityValidator()
    private let onProjection: (CleanupSessionProjection) -> Void
    private var lifecycle = Lifecycle.active
    private var volatileText: String?
    private var segments: [SegmentRecord] = []
    private var acceptedEventIDs: Set<String> = []
    private var pendingSequences: [Int] = []
    private var attemptsBySequence: [Int: AttemptTasks] = [:]
    private var nextPublicationSequence = 0
    private var drainTask: Task<Void, Never>?
    private var drainContinuations: [CheckedContinuation<CleanupTerminalSnapshot, Never>] = []

    init(
        sessionID: String,
        configuration: CleanupSessionConfiguration,
        provider: any CleanupProvider,
        context: TranscriptContext,
        protectedVocabulary: [String],
        onProjection: @escaping (CleanupSessionProjection) -> Void
    ) {
        self.sessionID = sessionID
        self.configuration = configuration
        self.provider = provider
        providerDescriptor = provider.descriptor
        self.context = configuration.contextPolicy.applying(to: context)
        self.protectedVocabulary = protectedVocabulary
        self.onProjection = onProjection
    }

    func updateVolatileText(_ text: String) {
        guard lifecycle == .active else { return }
        volatileText = text.trimmingCharacters(in: .whitespacesAndNewlines)
        publishProjection()
    }

    func acceptStableSegment(
        id: String,
        rawText: String,
        preparedText: String
    ) {
        guard lifecycle == .active,
              acceptedEventIDs.insert(id).inserted
        else {
            return
        }
        volatileText = nil
        let sequence = segments.count
        let normalizedRaw = rawText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalizedRaw.isEmpty else { return }
        let normalizedPrepared = preparedText.trimmingCharacters(in: .whitespacesAndNewlines)
        segments.append(SegmentRecord(
            id: id,
            sequence: sequence,
            rawText: normalizedRaw,
            preparedText: normalizedPrepared.isEmpty ? normalizedRaw : normalizedPrepared,
            state: .raw
        ))
        publishProjection()

        guard configuration.enabled else {
            segments[sequence].state = .published(nil)
            nextPublicationSequence += 1
            publishProjection()
            return
        }
        if attemptsBySequence.count < configuration.concurrencyLimit {
            startAttempt(sequence: sequence)
        } else if pendingSequences.count < configuration.pendingCapacity {
            segments[sequence].state = .queued
            pendingSequences.append(sequence)
            publishProjection()
        } else {
            segments[sequence].state = .waiting(.failure(.skippedCapacity))
            publishAvailableResults()
        }
    }

    func stopAndDrain() async -> CleanupTerminalSnapshot {
        switch lifecycle {
        case .finished, .cancelled:
            return terminalSnapshot()
        case .stopping:
            return await waitForDrain()
        case .active:
            lifecycle = .stopping
        }

        cancelQueuedSegments()
        publishAvailableResults()
        if attemptsBySequence.isEmpty {
            finishDrain()
            return terminalSnapshot()
        }

        drainTask = Task { [weak self, timeout = configuration.stopDrainTimeout] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            await MainActor.run { self?.forceDrainTimeout() }
        }
        return await waitForDrain()
    }

    func cancel() {
        guard lifecycle == .active || lifecycle == .stopping else { return }
        lifecycle = .cancelled
        drainTask?.cancel()
        drainTask = nil
        for attempt in attemptsBySequence.values {
            attempt.providerTask.cancel()
            attempt.timeoutTask.cancel()
        }
        attemptsBySequence.removeAll()
        pendingSequences.removeAll()
        for index in segments.indices {
            if !isPublished(segments[index].state) {
                segments[index].state = .published(.failure(.cancelled))
            }
        }
        nextPublicationSequence = segments.count
        publishProjection()
        resumeDrainContinuations()
    }

    private func startAttempt(sequence: Int) {
        guard segments.indices.contains(sequence),
              lifecycle == .active
        else {
            return
        }
        let attemptID = UUID()
        segments[sequence].state = .running(attemptID)
        let segment = segments[sequence]
        let request = CleanupRequest(
            sessionID: sessionID,
            segmentID: segment.id,
            segmentSequence: segment.sequence,
            rawText: configuration.contextPolicy.boundedSegment(segment.rawText),
            preparedText: configuration.contextPolicy.boundedSegment(segment.preparedText),
            context: context,
            protectedVocabulary: configuration.contextPolicy.boundedVocabulary(
                protectedVocabulary
            ),
            voiceProfile: configuration.voiceProfile,
            maximumResponseCharacters: configuration.contextPolicy.responseCharacterLimit
        )
        let providerTask = Task { [weak self, provider] in
            let result = await provider.cleanup(request)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                self?.completeAttempt(
                    sequence: sequence,
                    attemptID: attemptID,
                    result: result
                )
            }
        }
        let timeoutTask = Task { [weak self, timeout = configuration.attemptTimeout] in
            try? await Task.sleep(for: timeout)
            guard !Task.isCancelled else { return }
            await MainActor.run {
                self?.completeAttempt(
                    sequence: sequence,
                    attemptID: attemptID,
                    result: .failure(.timeout)
                )
            }
        }
        attemptsBySequence[sequence] = AttemptTasks(
            attemptID: attemptID,
            providerTask: providerTask,
            timeoutTask: timeoutTask
        )
        publishProjection()
    }

    private func completeAttempt(
        sequence: Int,
        attemptID: UUID,
        result: CleanupProviderResult
    ) {
        guard lifecycle == .active || lifecycle == .stopping,
              let tasks = attemptsBySequence[sequence],
              tasks.attemptID == attemptID,
              case .running(let currentAttemptID) = segments[sequence].state,
              currentAttemptID == attemptID
        else {
            return
        }
        tasks.providerTask.cancel()
        tasks.timeoutTask.cancel()
        attemptsBySequence.removeValue(forKey: sequence)
        segments[sequence].state = .waiting(validated(result, for: segments[sequence]))
        publishAvailableResults()

        if lifecycle == .active,
           !pendingSequences.isEmpty {
            let next = pendingSequences.removeFirst()
            startAttempt(sequence: next)
        }
        finishDrainIfPossible()
    }

    private func validated(
        _ result: CleanupProviderResult,
        for segment: SegmentRecord
    ) -> CleanupProviderResult {
        guard case .success(let value) = result else { return result }
        let text = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return .failure(.malformedOutput) }
        guard text.count <= configuration.contextPolicy.responseCharacterLimit else {
            return .failure(.outputTooLarge)
        }
        guard integrityValidator.violations(
            source: segment.preparedText,
            candidate: text,
            protectedVocabulary: protectedVocabulary
        ).isEmpty else {
            return .failure(.safety)
        }
        return .success(text)
    }

    private func publishAvailableResults() {
        while segments.indices.contains(nextPublicationSequence),
              case .waiting(let result) = segments[nextPublicationSequence].state {
            segments[nextPublicationSequence].state = .published(result)
            nextPublicationSequence += 1
        }
        publishProjection()
    }

    private func cancelQueuedSegments() {
        for sequence in pendingSequences {
            segments[sequence].state = .waiting(.failure(.cancelled))
        }
        pendingSequences.removeAll()
    }

    private func forceDrainTimeout() {
        guard lifecycle == .stopping else { return }
        for (sequence, attempt) in attemptsBySequence {
            attempt.providerTask.cancel()
            attempt.timeoutTask.cancel()
            segments[sequence].state = .waiting(.failure(.cancelled))
        }
        attemptsBySequence.removeAll()
        publishAvailableResults()
        finishDrain()
    }

    private func finishDrainIfPossible() {
        guard lifecycle == .stopping,
              attemptsBySequence.isEmpty,
              pendingSequences.isEmpty
        else {
            return
        }
        finishDrain()
    }

    private func finishDrain() {
        lifecycle = .finished
        drainTask?.cancel()
        drainTask = nil
        publishAvailableResults()
        resumeDrainContinuations()
    }

    private func resumeDrainContinuations() {
        let snapshot = terminalSnapshot()
        let continuations = drainContinuations
        drainContinuations.removeAll()
        continuations.forEach { $0.resume(returning: snapshot) }
    }

    private func waitForDrain() async -> CleanupTerminalSnapshot {
        await withCheckedContinuation { continuation in
            if lifecycle == .finished || lifecycle == .cancelled {
                continuation.resume(returning: terminalSnapshot())
            } else {
                drainContinuations.append(continuation)
            }
        }
    }

    private func terminalSnapshot() -> CleanupTerminalSnapshot {
        CleanupTerminalSnapshot(
            sessionID: sessionID,
            providerID: providerDescriptor.id,
            modelIdentifier: providerDescriptor.modelIdentifier,
            segments: segments.map { segment in
                let result: CleanupProviderResult?
                if case .published(let published) = segment.state {
                    result = published
                } else {
                    result = .failure(.cancelled)
                }
                switch result {
                case .success(let revised):
                    return CleanupTerminalSegment(
                        id: segment.id,
                        sequence: segment.sequence,
                        rawText: segment.rawText,
                        selectedText: revised,
                        failure: nil
                    )
                case .failure(let failure):
                    return CleanupTerminalSegment(
                        id: segment.id,
                        sequence: segment.sequence,
                        rawText: segment.rawText,
                        selectedText: segment.rawText,
                        failure: failure
                    )
                case nil:
                    return CleanupTerminalSegment(
                        id: segment.id,
                        sequence: segment.sequence,
                        rawText: segment.rawText,
                        selectedText: segment.preparedText,
                        failure: nil
                    )
                }
            }
        )
    }

    private func publishProjection() {
        onProjection(CleanupSessionProjection(
            cleanupProviderName: providerDescriptor.displayName,
            cleanupModelIdentifier: providerDescriptor.modelIdentifier,
            disclosedContextSources: context.sources,
            volatileText: volatileText,
            segments: segments.map(projection)
        ))
    }

    private func projection(_ segment: SegmentRecord) -> CleanupSegmentProjection {
        switch segment.state {
        case .raw:
            return makeProjection(segment, state: .raw)
        case .queued:
            return makeProjection(segment, state: .waiting)
        case .running:
            return makeProjection(segment, state: .rewriting)
        case .waiting:
            return makeProjection(segment, state: .waiting)
        case .published(.success(let text)):
            return makeProjection(segment, revisedText: text, state: .cleaned)
        case .published(.failure(let failure)):
            return makeProjection(
                segment,
                state: failure == .cancelled ? .cancelled : .fallback,
                failure: failure
            )
        case .published(nil):
            return makeProjection(segment, state: .raw)
        }
    }

    private func makeProjection(
        _ segment: SegmentRecord,
        revisedText: String? = nil,
        state: CleanupSegmentProjectionState,
        failure: CleanupProviderFailureCategory? = nil
    ) -> CleanupSegmentProjection {
        CleanupSegmentProjection(
            id: segment.id,
            sequence: segment.sequence,
            rawText: segment.rawText,
            revisedText: revisedText,
            state: state,
            failure: failure
        )
    }

    private func isPublished(_ state: OperationalState) -> Bool {
        if case .published = state { return true }
        return false
    }
}
