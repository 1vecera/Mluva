import Foundation

enum RollingAppleSpeechError: Error, LocalizedError {
    case alreadyStarted
    case audioBufferOverflow

    var errorDescription: String? {
        switch self {
        case .alreadyStarted:
            "Apple Speech is already running."
        case .audioBufferOverflow:
            "Apple Speech could not keep up with live audio. The recording remains available for retry."
        }
    }
}

final class RollingAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?

    private let rolloverAudioBytes: Int
    private let overlapAudioBytes: Int
    private let bufferedAudioChunks: Int
    private let makeEngine: () -> any AppleSpeechEngine
    private let lock = NSLock()
    private var audioContinuation: AsyncStream<Data>.Continuation?
    private var processingTask: Task<String, Error>?
    private var currentEngine: (any AppleSpeechEngine)?
    private var isStarted = false
    private var appendError: Error?

    init(
        rolloverAudioBytes: Int = 1_600_000,
        overlapAudioBytes: Int = 64_000,
        bufferedAudioChunks: Int = 512,
        makeEngine: @escaping () -> any AppleSpeechEngine = {
            LegacyAppleSpeechEngine()
        }
    ) {
        let evenRolloverBytes = max(2, rolloverAudioBytes - rolloverAudioBytes % 2)
        let evenOverlapBytes = max(0, overlapAudioBytes - overlapAudioBytes % 2)
        self.rolloverAudioBytes = evenRolloverBytes
        self.overlapAudioBytes = min(evenOverlapBytes, evenRolloverBytes / 2)
        self.bufferedAudioChunks = max(1, bufferedAudioChunks)
        self.makeEngine = makeEngine
    }

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        guard !lock.withLock({ isStarted }) else {
            throw RollingAppleSpeechError.alreadyStarted
        }

        let relay = RollingAppleTranscriptRelay { [weak self] text, isFinal in
            self?.onRecognition?(text, isFinal)
        }
        let firstEngine = makeEngine()
        connect(firstEngine, attemptID: "apple-roll-0", relay: relay)
        try await firstEngine.start(
            localeIdentifier: localeIdentifier,
            requiresOnDeviceRecognition: requiresOnDeviceRecognition,
            contextualStrings: contextualStrings
        )

        let (audio, continuation) = AsyncStream<Data>.makeStream(
            bufferingPolicy: .bufferingNewest(bufferedAudioChunks)
        )
        let configuration = AppleSpeechConfiguration(
            localeIdentifier: localeIdentifier,
            requiresOnDeviceRecognition: requiresOnDeviceRecognition,
            contextualStrings: contextualStrings
        )
        let task = Task {
            try await processAudio(
                audio,
                firstEngine: firstEngine,
                configuration: configuration,
                relay: relay
            )
        }
        lock.withLock {
            audioContinuation = continuation
            processingTask = task
            currentEngine = firstEngine
            appendError = nil
            isStarted = true
        }
    }

    func appendPCM16(_ data: Data) throws {
        guard !data.isEmpty, data.count.isMultiple(of: MemoryLayout<Int16>.size) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        let snapshot = lock.withLock {
            (isStarted, audioContinuation, appendError)
        }
        guard snapshot.0, let continuation = snapshot.1 else {
            throw AppleSpeechError.notStarted
        }
        if let appendError = snapshot.2 {
            throw appendError
        }

        switch continuation.yield(data) {
        case .enqueued:
            break
        case .dropped:
            let error = RollingAppleSpeechError.audioBufferOverflow
            lock.withLock { appendError = error }
            throw error
        case .terminated:
            throw AppleSpeechError.notStarted
        @unknown default:
            throw RollingAppleSpeechError.audioBufferOverflow
        }
    }

    func finish() async throws {
        let snapshot = lock.withLock {
            (isStarted, audioContinuation, processingTask)
        }
        guard snapshot.0, let task = snapshot.2 else {
            throw AppleSpeechError.notStarted
        }
        snapshot.1?.finish()

        do {
            let transcript = try await task.value
            if let appendError = lock.withLock({ appendError }) {
                throw appendError
            }
            let trimmed = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
            if !trimmed.isEmpty {
                onRecognition?(trimmed, true)
            }
            clearSession()
        } catch {
            lock.withLock { currentEngine }?.cancel()
            clearSession()
            throw error
        }
    }

    func cancel() {
        let snapshot = lock.withLock {
            let snapshot = (audioContinuation, processingTask, currentEngine)
            audioContinuation = nil
            processingTask = nil
            currentEngine = nil
            appendError = nil
            isStarted = false
            return snapshot
        }
        snapshot.0?.finish()
        snapshot.1?.cancel()
        snapshot.2?.cancel()
    }

    private func processAudio(
        _ audio: AsyncStream<Data>,
        firstEngine: any AppleSpeechEngine,
        configuration: AppleSpeechConfiguration,
        relay: RollingAppleTranscriptRelay
    ) async throws -> String {
        var engine = firstEngine
        var attemptIndex = 0
        var attemptID = "apple-roll-\(attemptIndex)"
        connect(engine, attemptID: attemptID, relay: relay)
        var attemptAudioBytes = 0
        var overlapAudio = Data()

        for await incomingAudio in audio {
            try Task.checkCancellation()
            var remainingAudio = incomingAudio
            while !remainingAudio.isEmpty {
                if attemptAudioBytes >= rolloverAudioBytes {
                    try await engine.finish()
                    _ = relay.finishAttempt(id: attemptID)
                    try Task.checkCancellation()

                    attemptIndex += 1
                    attemptID = "apple-roll-\(attemptIndex)"
                    engine = makeEngine()
                    connect(engine, attemptID: attemptID, relay: relay)
                    lock.withLock { currentEngine = engine }
                    try await engine.start(
                        localeIdentifier: configuration.localeIdentifier,
                        requiresOnDeviceRecognition: configuration.requiresOnDeviceRecognition,
                        contextualStrings: configuration.contextualStrings
                    )

                    let replayAudio = Data(overlapAudio.suffix(overlapAudioBytes))
                    if !replayAudio.isEmpty {
                        try engine.appendPCM16(replayAudio)
                    }
                    attemptAudioBytes = replayAudio.count
                    overlapAudio = replayAudio
                }

                let availableBytes = rolloverAudioBytes - attemptAudioBytes
                let chunkSize = min(availableBytes, remainingAudio.count)
                let chunkEnd = remainingAudio.index(
                    remainingAudio.startIndex,
                    offsetBy: chunkSize
                )
                let chunk = Data(remainingAudio[..<chunkEnd])
                try engine.appendPCM16(chunk)
                attemptAudioBytes += chunk.count
                overlapAudio.append(chunk)
                if overlapAudio.count > overlapAudioBytes {
                    overlapAudio.removeFirst(overlapAudio.count - overlapAudioBytes)
                }
                remainingAudio = Data(remainingAudio[chunkEnd...])
            }
        }

        try await engine.finish()
        return relay.finishAttempt(id: attemptID)
    }

    private func connect(
        _ engine: any AppleSpeechEngine,
        attemptID: String,
        relay: RollingAppleTranscriptRelay
    ) {
        engine.onRecognition = { text, isFinal in
            relay.receive(text, isFinal: isFinal, attemptID: attemptID)
        }
    }

    private func clearSession() {
        lock.withLock {
            audioContinuation = nil
            processingTask = nil
            currentEngine = nil
            appendError = nil
            isStarted = false
        }
    }
}

private final class RollingAppleTranscriptRelay: @unchecked Sendable {
    private let lock = NSLock()
    private let handler: (String, Bool) -> Void
    private var transcript = TranscriptAccumulator()
    private var latestTextByAttempt: [String: String] = [:]

    init(handler: @escaping (String, Bool) -> Void) {
        self.handler = handler
    }

    func receive(_ text: String, isFinal: Bool, attemptID: String) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let displayText = lock.withLock { () -> String in
            latestTextByAttempt[attemptID] = trimmed
            transcript.ingest(isFinal
                ? .final(id: attemptID, text: trimmed)
                : .volatile(id: attemptID, text: trimmed))
            return transcript.displayText
        }
        handler(displayText, false)
    }

    func finishAttempt(id: String) -> String {
        lock.withLock {
            if let latestText = latestTextByAttempt[id] {
                transcript.ingest(.final(id: id, text: latestText))
            }
            return transcript.finalText
        }
    }
}
