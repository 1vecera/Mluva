import AVFoundation
import Foundation
import Speech

struct AppleSpeechConfiguration: Equatable, Sendable {
    let localeIdentifier: String
    let requiresOnDeviceRecognition: Bool
    let contextualStrings: [String]

    init(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool = true,
        contextualStrings: [String] = []
    ) {
        self.localeIdentifier = localeIdentifier
        self.requiresOnDeviceRecognition = requiresOnDeviceRecognition
        self.contextualStrings = contextualStrings
    }
}

enum AppleSpeechError: Error, LocalizedError {
    case permissionDenied
    case recognizerUnavailable(String)
    case onDeviceRecognitionUnavailable(String)
    case invalidAudioChunk
    case notStarted
    case modelInstallationFailed(String)
    case finalizationTimedOut

    var errorDescription: String? {
        switch self {
        case .permissionDenied:
            return "Speech recognition permission is required for Apple transcription."
        case .recognizerUnavailable(let locale):
            return "Apple Speech is unavailable for \(locale)."
        case .onDeviceRecognitionUnavailable(let locale):
            return "On-device Apple Speech is unavailable for \(locale)."
        case .invalidAudioChunk:
            return "The captured audio chunk is not valid 16-bit PCM."
        case .notStarted:
            return "Apple Speech has not been started."
        case .modelInstallationFailed(let locale):
            return "Apple's on-device speech model could not be installed for \(locale)."
        case .finalizationTimedOut:
            return "Apple Speech did not finish the transcript in time."
        }
    }
}

protocol AppleSpeechEngine: AnyObject {
    var onRecognition: ((String, Bool) -> Void)? { get set }

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws
    func appendPCM16(_ data: Data) throws
    func finish() async throws
    func cancel()
}

final class AppleSpeechTranscriptionProvider: TranscriptionProvider {
    let kind: TranscriptionProviderKind = .apple
    var onEvent: ((TranscriptEvent) -> Void)?

    private let configuration: AppleSpeechConfiguration
    private let engine: any AppleSpeechEngine
    private let makeEventID: () -> String
    private let lock = NSLock()
    private var eventID = ""
    private var latestText = ""
    private var appendError: Error?

    init(
        configuration: AppleSpeechConfiguration,
        engine: any AppleSpeechEngine = SystemAppleSpeechEngine(),
        makeEventID: @escaping () -> String = { UUID().uuidString }
    ) {
        self.configuration = configuration
        self.engine = engine
        self.makeEventID = makeEventID
    }

    func start() async throws {
        let newEventID = makeEventID()
        lock.withLock {
            eventID = newEventID
            latestText = ""
            appendError = nil
        }

        engine.onRecognition = { [weak self] text, isFinal in
            self?.receiveRecognition(text, isFinal: isFinal)
        }
        try await engine.start(
            localeIdentifier: configuration.localeIdentifier,
            requiresOnDeviceRecognition: configuration.requiresOnDeviceRecognition,
            contextualStrings: configuration.contextualStrings
        )
    }

    func appendAudio(_ data: Data) {
        do {
            try engine.appendPCM16(data)
        } catch {
            lock.withLock {
                appendError = error
            }
        }
    }

    func finish() async throws -> String {
        if let error = lock.withLock({ appendError }) {
            throw error
        }
        try await engine.finish()
        if let error = lock.withLock({ appendError }) {
            throw error
        }
        return lock.withLock { latestText }
    }

    func cancel() {
        engine.cancel()
    }

    private func receiveRecognition(_ text: String, isFinal: Bool) {
        let trimmed = text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }

        let currentEventID = lock.withLock { () -> String in
            latestText = trimmed
            return eventID
        }
        let event = isFinal
            ? TranscriptEvent.final(id: currentEventID, text: trimmed)
            : TranscriptEvent.volatile(id: currentEventID, text: trimmed)
        onEvent?(event)
    }
}

final class LegacyAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?

    private let lock = NSLock()
    private var recognizer: SFSpeechRecognizer?
    private var request: SFSpeechAudioBufferRecognitionRequest?
    private var task: SFSpeechRecognitionTask?
    private var finishContinuation: CheckedContinuation<Void, Error>?
    private var terminalResult: Result<Void, Error>?

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        guard SFSpeechRecognizer.authorizationStatus() == .authorized else {
            throw AppleSpeechError.permissionDenied
        }

        let locale = Locale(identifier: localeIdentifier)
        guard let recognizer = SFSpeechRecognizer(locale: locale), recognizer.isAvailable else {
            throw AppleSpeechError.recognizerUnavailable(localeIdentifier)
        }
        if requiresOnDeviceRecognition, !recognizer.supportsOnDeviceRecognition {
            throw AppleSpeechError.onDeviceRecognitionUnavailable(localeIdentifier)
        }

        let request = SFSpeechAudioBufferRecognitionRequest()
        request.shouldReportPartialResults = true
        request.requiresOnDeviceRecognition = requiresOnDeviceRecognition
        request.contextualStrings = contextualStrings

        lock.withLock {
            self.recognizer = recognizer
            self.request = request
            terminalResult = nil
            finishContinuation = nil
        }

        task = recognizer.recognitionTask(with: request) { [weak self] result, error in
            if let result {
                self?.onRecognition?(result.bestTranscription.formattedString, result.isFinal)
                if result.isFinal {
                    self?.resolveFinish(.success(()))
                }
            }
            if let error {
                self?.resolveFinish(.failure(error))
            }
        }
    }

    func appendPCM16(_ data: Data) throws {
        guard !data.isEmpty, data.count.isMultiple(of: MemoryLayout<Int16>.size) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        guard let request = lock.withLock({ request }) else {
            throw AppleSpeechError.notStarted
        }
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: true
        ) else {
            throw AppleSpeechError.invalidAudioChunk
        }

        let frameCount = AVAudioFrameCount(data.count / MemoryLayout<Int16>.size)
        guard let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frameCount) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        buffer.frameLength = frameCount

        let audioBuffers = UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList)
        guard let destination = audioBuffers.first?.mData else {
            throw AppleSpeechError.invalidAudioChunk
        }
        data.copyBytes(to: destination.assumingMemoryBound(to: UInt8.self), count: data.count)
        audioBuffers[0].mDataByteSize = UInt32(data.count)
        request.append(buffer)
    }

    func finish() async throws {
        guard let request = lock.withLock({ request }) else {
            throw AppleSpeechError.notStarted
        }
        request.endAudio()
        task?.finish()

        Task { [weak self] in
            try? await Task.sleep(for: .seconds(8))
            self?.resolveFinish(.failure(AppleSpeechError.finalizationTimedOut))
        }

        let existingResult = lock.withLock { terminalResult }
        if let existingResult {
            return try existingResult.get()
        }

        try await withCheckedThrowingContinuation { continuation in
            var resumeImmediately: Result<Void, Error>?
            lock.withLock {
                if let terminalResult {
                    resumeImmediately = terminalResult
                } else {
                    finishContinuation = continuation
                }
            }
            if let resumeImmediately {
                continuation.resume(with: resumeImmediately)
            }
        }
    }

    func cancel() {
        task?.cancel()
        request?.endAudio()
        resolveFinish(.failure(CancellationError()))
        clearSession()
    }

    private func resolveFinish(_ result: Result<Void, Error>) {
        var continuation: CheckedContinuation<Void, Error>?
        lock.withLock {
            guard terminalResult == nil else { return }
            terminalResult = result
            continuation = finishContinuation
            finishContinuation = nil
        }
        continuation?.resume(with: result)
    }

    private func clearSession() {
        lock.withLock {
            recognizer = nil
            request = nil
            task = nil
        }
    }
}

private extension NSLock {
    func withLock<T>(_ operation: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try operation()
    }
}
