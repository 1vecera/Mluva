import AVFoundation
import Foundation
import Speech

final class SystemAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)? {
        didSet { backend.onRecognition = onRecognition }
    }

    private let backend: any AppleSpeechEngine

    init() {
        if #available(macOS 26.0, *) {
            backend = SpeechAnalyzerAppleSpeechEngine()
        } else {
            backend = RollingAppleSpeechEngine()
        }
    }

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        backend.onRecognition = onRecognition
        try await backend.start(
            localeIdentifier: localeIdentifier,
            requiresOnDeviceRecognition: requiresOnDeviceRecognition,
            contextualStrings: contextualStrings
        )
    }

    func appendPCM16(_ data: Data) throws {
        try backend.appendPCM16(data)
    }

    func finish() async throws {
        try await backend.finish()
    }

    func cancel() {
        backend.cancel()
    }
}

@available(macOS 26.0, *)
private final class SpeechAnalyzerAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?

    private let lock = NSLock()
    private var analyzer: SpeechAnalyzer?
    private var inputContinuation: AsyncStream<AnalyzerInput>.Continuation?
    private var resultTask: Task<Void, Error>?
    private var analyzerFormat: AVAudioFormat?
    private var converter: AVAudioConverter?
    private var isStarted = false

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        guard SFSpeechRecognizer.authorizationStatus() == .authorized else {
            throw AppleSpeechError.permissionDenied
        }

        let requestedLocale = Locale(identifier: localeIdentifier)
        guard let supportedLocale = await SpeechTranscriber.supportedLocale(
            equivalentTo: requestedLocale
        ) else {
            throw AppleSpeechError.recognizerUnavailable(localeIdentifier)
        }

        let transcriber = SpeechTranscriber(
            locale: supportedLocale,
            transcriptionOptions: [],
            reportingOptions: [.volatileResults],
            attributeOptions: []
        )
        try await installModelIfNeeded(for: transcriber, localeIdentifier: localeIdentifier)

        guard let format = await SpeechAnalyzer.bestAvailableAudioFormat(
            compatibleWith: [transcriber]
        ) else {
            throw AppleSpeechError.onDeviceRecognitionUnavailable(localeIdentifier)
        }

        let context = AnalysisContext()
        context.contextualStrings[.general] = contextualStrings
        let analyzer = SpeechAnalyzer(modules: [transcriber])
        try await analyzer.setContext(context)

        let (inputSequence, continuation) = AsyncStream<AnalyzerInput>.makeStream()
        let handler = RecognitionHandler { [weak self] text, isFinal in
            self?.onRecognition?(text, isFinal)
        }
        let resultTask = Task {
            var finalSegments: [String] = []
            for try await result in transcriber.results {
                let text = String(result.text.characters)
                    .trimmingCharacters(in: .whitespacesAndNewlines)
                guard !text.isEmpty else { continue }

                if result.isFinal {
                    finalSegments.append(text)
                    handler.send(finalSegments.joined(separator: " "), isFinal: true)
                } else {
                    handler.send(
                        (finalSegments + [text]).joined(separator: " "),
                        isFinal: false
                    )
                }
            }
        }

        lock.withLock {
            self.analyzer = analyzer
            inputContinuation = continuation
            self.resultTask = resultTask
            analyzerFormat = format
            converter = nil
            isStarted = true
        }
        try await analyzer.start(inputSequence: inputSequence)
    }

    func appendPCM16(_ data: Data) throws {
        guard !data.isEmpty, data.count.isMultiple(of: MemoryLayout<Int16>.size) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        let state = lock.withLock {
            (inputContinuation, analyzerFormat, isStarted)
        }
        guard state.2, let continuation = state.0, let format = state.1 else {
            throw AppleSpeechError.notStarted
        }

        let inputBuffer = try Self.makePCMBuffer(data)
        let convertedBuffer = try convert(inputBuffer, to: format)
        continuation.yield(AnalyzerInput(buffer: convertedBuffer))
    }

    func finish() async throws {
        let state = lock.withLock {
            (analyzer, inputContinuation, resultTask, isStarted)
        }
        guard state.3, let analyzer = state.0 else {
            throw AppleSpeechError.notStarted
        }

        state.1?.finish()
        try await analyzer.finalizeAndFinishThroughEndOfInput()
        state.2?.cancel()
        clearSession()
    }

    func cancel() {
        let state = lock.withLock { (analyzer, inputContinuation, resultTask) }
        state.1?.finish()
        state.2?.cancel()
        if let analyzer = state.0 {
            Task { await analyzer.cancelAndFinishNow() }
        }
        clearSession()
    }

    private func installModelIfNeeded(
        for transcriber: SpeechTranscriber,
        localeIdentifier: String
    ) async throws {
        let installedLocales = await SpeechTranscriber.installedLocales
        guard !installedLocales.contains(where: {
            $0.identifier(.bcp47) == transcriber.selectedLocales.first?.identifier(.bcp47)
        }) else {
            return
        }

        guard let request = try await AssetInventory.assetInstallationRequest(
            supporting: [transcriber]
        ) else {
            throw AppleSpeechError.modelInstallationFailed(localeIdentifier)
        }
        try await request.downloadAndInstall()
    }

    private func convert(
        _ buffer: AVAudioPCMBuffer,
        to format: AVAudioFormat
    ) throws -> AVAudioPCMBuffer {
        guard buffer.format != format else { return buffer }

        let converter = try lock.withLock { () throws -> AVAudioConverter in
            if let existingConverter = self.converter,
               existingConverter.outputFormat == format {
                return existingConverter
            }
            guard let newConverter = AVAudioConverter(from: buffer.format, to: format) else {
                throw AppleSpeechError.invalidAudioChunk
            }
            newConverter.primeMethod = .none
            self.converter = newConverter
            return newConverter
        }

        let ratio = converter.outputFormat.sampleRate / converter.inputFormat.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up))
        guard let output = AVAudioPCMBuffer(
            pcmFormat: converter.outputFormat,
            frameCapacity: capacity
        ) else {
            throw AppleSpeechError.invalidAudioChunk
        }

        var conversionError: NSError?
        var suppliedInput = false
        let status = converter.convert(to: output, error: &conversionError) { _, inputStatus in
            if suppliedInput {
                inputStatus.pointee = .noDataNow
                return nil
            }
            suppliedInput = true
            inputStatus.pointee = .haveData
            return buffer
        }
        guard status != .error, conversionError == nil else {
            throw conversionError ?? AppleSpeechError.invalidAudioChunk
        }
        return output
    }

    private static func makePCMBuffer(_ data: Data) throws -> AVAudioPCMBuffer {
        guard let format = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16_000,
            channels: 1,
            interleaved: true
        ) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        let frameCount = AVAudioFrameCount(data.count / MemoryLayout<Int16>.size)
        guard let buffer = AVAudioPCMBuffer(
            pcmFormat: format,
            frameCapacity: frameCount
        ) else {
            throw AppleSpeechError.invalidAudioChunk
        }
        buffer.frameLength = frameCount

        let buffers = UnsafeMutableAudioBufferListPointer(buffer.mutableAudioBufferList)
        guard let destination = buffers.first?.mData else {
            throw AppleSpeechError.invalidAudioChunk
        }
        data.copyBytes(to: destination.assumingMemoryBound(to: UInt8.self), count: data.count)
        buffers[0].mDataByteSize = UInt32(data.count)
        return buffer
    }

    private func clearSession() {
        lock.withLock {
            analyzer = nil
            inputContinuation = nil
            resultTask = nil
            analyzerFormat = nil
            converter = nil
            isStarted = false
        }
    }
}

private final class RecognitionHandler: @unchecked Sendable {
    private let handler: (String, Bool) -> Void

    init(handler: @escaping (String, Bool) -> Void) {
        self.handler = handler
    }

    func send(_ text: String, isFinal: Bool) {
        handler(text, isFinal)
    }
}
