import Foundation

struct GoogleCloudSpeechConfiguration: Equatable, Sendable {
    let projectID: String
    let location: String
    let recognizer: String
    let model: String
    let languageCodes: [String]
    let maximumAudioBytes: Int
    let streamingRolloverAudioBytes: Int
    let streamingOverlapAudioBytes: Int

    init(
        projectID: String,
        location: String = "eu",
        recognizer: String = "_",
        model: String = "chirp_3",
        languageCodes: [String] = ["en-US"],
        maximumAudioBytes: Int = 1_920_000,
        streamingRolloverAudioBytes: Int = 9_120_000,
        streamingOverlapAudioBytes: Int = 32_000
    ) {
        self.projectID = projectID
        self.location = location
        self.recognizer = recognizer
        self.model = model
        self.languageCodes = languageCodes
        self.maximumAudioBytes = maximumAudioBytes
        self.streamingRolloverAudioBytes = streamingRolloverAudioBytes
        self.streamingOverlapAudioBytes = streamingOverlapAudioBytes
    }

    var endpoint: URL? {
        URL(string: "https://\(location)-speech.googleapis.com/v2/projects/\(projectID)/locations/\(location)/recognizers/\(recognizer):recognize")
    }

    var recognizerPath: String {
        "projects/\(projectID)/locations/\(location)/recognizers/\(recognizer)"
    }

    var streamingEndpointHost: String {
        "\(location)-speech.googleapis.com"
    }

    var streamingConfiguration: GoogleStreamingRecognitionConfiguration {
        GoogleStreamingRecognitionConfiguration(
            recognizer: recognizerPath,
            model: model,
            languageCodes: languageCodes,
            sampleRateHertz: 16_000,
            audioChannelCount: 1,
            interimResults: true,
            automaticPunctuation: true
        )
    }
}

enum GoogleCloudSpeechError: Error, Equatable, LocalizedError {
    case invalidConfiguration
    case notStarted
    case emptyAudio
    case audioTooLong
    case authenticationUnavailable
    case connectionTimeout
    case invalidServiceAccountFile
    case invalidResponse
    case server(statusCode: Int)

    var errorDescription: String? {
        switch self {
        case .invalidConfiguration:
            return "Google Cloud Speech configuration is incomplete."
        case .notStarted:
            return "Google Cloud transcription has not been started."
        case .emptyAudio:
            return "No speech audio was captured."
        case .audioTooLong:
            return "This recording is too long for synchronous Google recognition."
        case .authenticationUnavailable:
            return "Google credentials are unavailable. Configure Application Default Credentials or select a valid service-account JSON file."
        case .connectionTimeout:
            return "Google Cloud Speech did not become ready in time. Check the connection and try again."
        case .invalidServiceAccountFile:
            return "The selected Google service-account JSON file is invalid or unreadable."
        case .invalidResponse:
            return "Google Cloud Speech returned an invalid response."
        case .server(let statusCode):
            return "Google Cloud Speech failed with HTTP status \(statusCode)."
        }
    }
}

protocol GoogleAccessTokenProviding: Sendable {
    func accessToken() async throws -> String
}

protocol HTTPDataTransport: Sendable {
    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse)
}

struct URLSessionHTTPDataTransport: HTTPDataTransport {
    private let session: URLSession

    init(session: URLSession = .shared) {
        self.session = session
    }

    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        let (data, response) = try await session.data(for: request)
        guard let httpResponse = response as? HTTPURLResponse else {
            throw GoogleCloudSpeechError.invalidResponse
        }
        return (data, httpResponse)
    }
}

struct GCloudExecutableLocator: Sendable {
    let environmentPath: String
    let knownURLs: [URL]

    init(
        environmentPath: String = ProcessInfo.processInfo.environment["PATH"] ?? "",
        knownURLs: [URL] = Self.defaultKnownURLs
    ) {
        self.environmentPath = environmentPath
        self.knownURLs = knownURLs
    }

    func locate() -> URL? {
        let pathURLs = environmentPath
            .split(separator: ":")
            .map { URL(fileURLWithPath: String($0), isDirectory: true) }
            .map { $0.appendingPathComponent("gcloud") }
        return (pathURLs + knownURLs).first {
            FileManager.default.isExecutableFile(atPath: $0.path)
        }
    }

    private static var defaultKnownURLs: [URL] {
        let home = FileManager.default.homeDirectoryForCurrentUser
        return [
            URL(fileURLWithPath: "/opt/homebrew/bin/gcloud"),
            URL(fileURLWithPath: "/opt/homebrew/share/google-cloud-sdk/bin/gcloud"),
            URL(fileURLWithPath: "/usr/local/bin/gcloud"),
            URL(fileURLWithPath: "/usr/local/google-cloud-sdk/bin/gcloud"),
            home.appendingPathComponent("google-cloud-sdk/bin/gcloud"),
        ]
    }
}

struct GCloudApplicationDefaultCredentialsTokenProvider: GoogleAccessTokenProviding {
    private let executableLocator: GCloudExecutableLocator

    init(executableLocator: GCloudExecutableLocator = GCloudExecutableLocator()) {
        self.executableLocator = executableLocator
    }

    func accessToken() async throws -> String {
        guard let executableURL = executableLocator.locate() else {
            throw GoogleCloudSpeechError.authenticationUnavailable
        }

        return try await withCheckedThrowingContinuation { continuation in
            let process = Process()
            let standardOutput = Pipe()
            process.executableURL = executableURL
            process.arguments = ["auth", "application-default", "print-access-token", "--quiet"]
            process.standardOutput = standardOutput
            process.standardError = Pipe()
            process.terminationHandler = { process in
                guard process.terminationStatus == 0 else {
                    continuation.resume(throwing: GoogleCloudSpeechError.authenticationUnavailable)
                    return
                }
                let data = standardOutput.fileHandleForReading.readDataToEndOfFile()
                let token = String(data: data, encoding: .utf8)?
                    .trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
                guard !token.isEmpty else {
                    continuation.resume(throwing: GoogleCloudSpeechError.authenticationUnavailable)
                    return
                }
                continuation.resume(returning: token)
            }

            do {
                try process.run()
            } catch {
                continuation.resume(throwing: GoogleCloudSpeechError.authenticationUnavailable)
            }
        }
    }
}

final class GoogleCloudTranscriptionProvider: TranscriptionProvider {
    let kind: TranscriptionProviderKind = .googleCloud
    var onEvent: ((TranscriptEvent) -> Void)? {
        get { eventRelay.handler }
        set { eventRelay.handler = newValue }
    }

    private let configuration: GoogleCloudSpeechConfiguration
    private let tokenProvider: any GoogleAccessTokenProviding
    private let transport: any HTTPDataTransport
    private let streamingTransport: (any GoogleStreamingRecognitionTransport)?
    private let makeEventID: () -> String
    private let eventRelay = TranscriptEventRelay()
    private let lock = NSLock()
    private var audio = Data()
    private var eventID = ""
    private var started = false
    private var streamingContinuation: AsyncStream<Data>.Continuation?
    private var streamingTask: Task<String, Error>?

    init(
        configuration: GoogleCloudSpeechConfiguration,
        tokenProvider: any GoogleAccessTokenProviding = GCloudApplicationDefaultCredentialsTokenProvider(),
        transport: any HTTPDataTransport = URLSessionHTTPDataTransport(),
        streamingTransport: (any GoogleStreamingRecognitionTransport)? = nil,
        makeEventID: @escaping () -> String = { UUID().uuidString }
    ) {
        self.configuration = configuration
        self.tokenProvider = tokenProvider
        self.transport = transport
        self.streamingTransport = streamingTransport
        self.makeEventID = makeEventID
    }

    func start() async throws {
        guard !configuration.projectID.isEmpty,
              !configuration.location.isEmpty,
              !configuration.languageCodes.isEmpty,
              configuration.streamingRolloverAudioBytes > 0,
              configuration.streamingOverlapAudioBytes >= 0,
              configuration.endpoint != nil
        else {
            throw GoogleCloudSpeechError.invalidConfiguration
        }

        let accessToken: String?
        if streamingTransport != nil {
            accessToken = try await tokenProvider.accessToken()
        } else {
            accessToken = nil
        }

        lock.withLock {
            streamingContinuation?.finish()
            streamingTask?.cancel()
            audio = Data()
            eventID = makeEventID()
            started = true
            streamingContinuation = nil
            streamingTask = nil
        }

        if let streamingTransport, let accessToken {
            let readiness = GoogleStreamingReadinessSignal()
            beginStreamingRun(
                transport: streamingTransport,
                accessToken: accessToken,
                replayAudio: nil,
                onReady: {
                    Task { await readiness.succeed() }
                },
                onFailureBeforeReady: { error in
                    Task { await readiness.fail(error) }
                }
            )
            let timeoutTask = Task {
                try? await Task.sleep(for: .seconds(15))
                guard !Task.isCancelled else { return }
                await readiness.fail(GoogleCloudSpeechError.connectionTimeout)
            }
            defer { timeoutTask.cancel() }
            do {
                try await readiness.wait()
            } catch {
                cancel()
                throw error
            }
        }
    }

    func appendAudio(_ data: Data) {
        let continuation = lock.withLock {
            guard started else { return nil as AsyncStream<Data>.Continuation? }
            audio.append(data)
            return streamingContinuation
        }
        continuation?.yield(data)
    }

    func finish() async throws -> String {
        if streamingTransport != nil {
            return try await finishStreamingAttempt()
        }
        return try await recognizeBufferedAudio()
    }

    func retry() async throws -> String {
        if let streamingTransport {
            let snapshot = lock.withLock { (audio, started) }
            guard snapshot.1 else { throw GoogleCloudSpeechError.notStarted }
            guard !snapshot.0.isEmpty else { throw GoogleCloudSpeechError.emptyAudio }
            let accessToken = try await tokenProvider.accessToken()
            beginStreamingRun(
                transport: streamingTransport,
                accessToken: accessToken,
                replayAudio: snapshot.0,
                onReady: {},
                onFailureBeforeReady: { _ in }
            )
            return try await finishStreamingAttempt()
        }
        return try await recognizeBufferedAudio()
    }

    func cancel() {
        let active = lock.withLock { () -> (
            AsyncStream<Data>.Continuation?,
            Task<String, Error>?
        ) in
            let active = (streamingContinuation, streamingTask)
            audio = Data()
            started = false
            streamingContinuation = nil
            streamingTask = nil
            return active
        }
        active.0?.finish()
        active.1?.cancel()
    }

    private func beginStreamingRun(
        transport: any GoogleStreamingRecognitionTransport,
        accessToken: String,
        replayAudio: Data?,
        onReady: @escaping @Sendable () -> Void,
        onFailureBeforeReady: @escaping @Sendable (any Error) -> Void
    ) {
        let (audioInput, continuation) = AsyncStream<Data>.makeStream()
        let task = Task {
            try await recognizeStreamingAudio(
                audioInput,
                transport: transport,
                accessToken: accessToken,
                onReady: onReady,
                onFailureBeforeReady: onFailureBeforeReady
            )
        }

        lock.withLock {
            streamingContinuation?.finish()
            streamingTask?.cancel()
            streamingContinuation = continuation
            streamingTask = task
        }
        if let replayAudio {
            continuation.yield(replayAudio)
        }
    }

    private func recognizeStreamingAudio(
        _ audioInput: AsyncStream<Data>,
        transport: any GoogleStreamingRecognitionTransport,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onFailureBeforeReady: @escaping @Sendable (any Error) -> Void
    ) async throws -> String {
        let rolloverBytes = configuration.streamingRolloverAudioBytes
        let overlapBytes = min(
            configuration.streamingOverlapAudioBytes,
            max(0, rolloverBytes / 2)
        )
        var attempt = makeStreamingAttempt(
            transport: transport,
            accessToken: accessToken,
            onReady: onReady,
            onFailureBeforeReady: onFailureBeforeReady
        )
        var attemptAudioBytes = 0
        var overlapAudio = Data()
        var transcript = TranscriptAccumulator()
        var streamID = 0

        for await incomingAudio in audioInput {
            var remainingAudio = incomingAudio
            while !remainingAudio.isEmpty {
                if attemptAudioBytes >= rolloverBytes {
                    attempt.continuation.finish()
                    let completedTranscript = try await attempt.task.value
                    transcript.ingest(
                        .final(id: "google-stream-\(streamID)", text: completedTranscript)
                    )
                    streamID += 1

                    attempt = makeStreamingAttempt(
                        transport: transport,
                        accessToken: accessToken,
                        onReady: {},
                        onFailureBeforeReady: { _ in }
                    )
                    let replayAudio = Data(overlapAudio.suffix(overlapBytes))
                    for chunk in replayAudio.chunks(maximumBytes: 15_000) {
                        attempt.continuation.yield(.audio(chunk))
                    }
                    attemptAudioBytes = replayAudio.count
                    overlapAudio = replayAudio
                }

                let availableBytes = rolloverBytes - attemptAudioBytes
                let chunkSize = min(15_000, min(availableBytes, remainingAudio.count))
                let chunkEnd = remainingAudio.index(
                    remainingAudio.startIndex,
                    offsetBy: chunkSize
                )
                let chunk = Data(remainingAudio[..<chunkEnd])
                attempt.continuation.yield(.audio(chunk))
                attemptAudioBytes += chunk.count
                overlapAudio.append(chunk)
                if overlapAudio.count > overlapBytes {
                    overlapAudio.removeFirst(overlapAudio.count - overlapBytes)
                }
                remainingAudio = Data(remainingAudio[chunkEnd...])
            }
        }

        attempt.continuation.finish()
        let completedTranscript = try await attempt.task.value
        transcript.ingest(.final(id: "google-stream-\(streamID)", text: completedTranscript))
        return transcript.finalText
    }

    private func makeStreamingAttempt(
        transport: any GoogleStreamingRecognitionTransport,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onFailureBeforeReady: @escaping @Sendable (any Error) -> Void
    ) -> GoogleStreamingAttempt {
        let (requests, continuation) = AsyncStream<GoogleStreamingRequest>.makeStream()
        let eventRelay = eventRelay
        let endpointHost = configuration.streamingEndpointHost
        let task = Task {
            do {
                return try await transport.recognize(
                    endpointHost: endpointHost,
                    requests: requests,
                    accessToken: accessToken,
                    onReady: onReady,
                    onEvent: { event in eventRelay.send(event) }
                )
            } catch {
                onFailureBeforeReady(error)
                throw error
            }
        }
        continuation.yield(.configuration(configuration.streamingConfiguration))
        return GoogleStreamingAttempt(continuation: continuation, task: task)
    }

    private func finishStreamingAttempt() async throws -> String {
        let snapshot = lock.withLock {
            (audio, started, streamingContinuation, streamingTask)
        }
        guard snapshot.1 else { throw GoogleCloudSpeechError.notStarted }
        guard !snapshot.0.isEmpty else {
            snapshot.2?.finish()
            snapshot.3?.cancel()
            throw GoogleCloudSpeechError.emptyAudio
        }
        guard let task = snapshot.3 else { throw GoogleCloudSpeechError.notStarted }

        snapshot.2?.finish()
        let transcript = try await task.value
        lock.withLock {
            started = false
            streamingContinuation = nil
            streamingTask = nil
        }
        return transcript
    }

    private func recognizeBufferedAudio() async throws -> String {
        let snapshot = lock.withLock { (audio, eventID, started) }
        guard snapshot.2 else { throw GoogleCloudSpeechError.notStarted }
        guard !snapshot.0.isEmpty else { throw GoogleCloudSpeechError.emptyAudio }
        guard snapshot.0.count <= configuration.maximumAudioBytes else {
            throw GoogleCloudSpeechError.audioTooLong
        }
        guard let endpoint = configuration.endpoint else {
            throw GoogleCloudSpeechError.invalidConfiguration
        }

        let token = try await tokenProvider.accessToken()
        let body = RecognitionRequestBody(
            config: .init(
                explicitDecodingConfig: .init(
                    encoding: "LINEAR16",
                    sampleRateHertz: 16_000,
                    audioChannelCount: 1
                ),
                languageCodes: configuration.languageCodes,
                model: configuration.model,
                features: .init(enableAutomaticPunctuation: true)
            ),
            content: snapshot.0.base64EncodedString()
        )

        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue("application/json", forHTTPHeaderField: "Content-Type")
        request.httpBody = try JSONEncoder().encode(body)

        let (data, response) = try await transport.data(for: request)
        guard (200..<300).contains(response.statusCode) else {
            throw GoogleCloudSpeechError.server(statusCode: response.statusCode)
        }

        guard let decoded = try? JSONDecoder().decode(RecognitionResponseBody.self, from: data) else {
            throw GoogleCloudSpeechError.invalidResponse
        }
        let transcript = decoded.results
            .compactMap { $0.alternatives.first?.transcript }
            .map { $0.trimmingCharacters(in: .whitespacesAndNewlines) }
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        guard !transcript.isEmpty else {
            throw GoogleCloudSpeechError.invalidResponse
        }

        onEvent?(.final(id: snapshot.1, text: transcript))
        return transcript
    }
}

private actor GoogleStreamingReadinessSignal {
    private var result: Result<Void, any Error>?
    private var continuations: [CheckedContinuation<Void, any Error>] = []

    func wait() async throws {
        if let result {
            return try result.get()
        }
        try await withCheckedThrowingContinuation { continuation in
            continuations.append(continuation)
        }
    }

    func succeed() {
        resolve(.success(()))
    }

    func fail(_ error: any Error) {
        resolve(.failure(error))
    }

    private func resolve(_ result: Result<Void, any Error>) {
        guard self.result == nil else { return }
        self.result = result
        let pending = continuations
        continuations.removeAll()
        pending.forEach { continuation in
            switch result {
            case .success:
                continuation.resume()
            case .failure(let error):
                continuation.resume(throwing: error)
            }
        }
    }
}

private struct RecognitionRequestBody: Encodable {
    struct RecognitionConfig: Encodable {
        struct ExplicitDecodingConfig: Encodable {
            let encoding: String
            let sampleRateHertz: Int
            let audioChannelCount: Int
        }

        struct RecognitionFeatures: Encodable {
            let enableAutomaticPunctuation: Bool
        }

        let explicitDecodingConfig: ExplicitDecodingConfig
        let languageCodes: [String]
        let model: String
        let features: RecognitionFeatures
    }

    let config: RecognitionConfig
    let content: String
}

private struct GoogleStreamingAttempt {
    let continuation: AsyncStream<GoogleStreamingRequest>.Continuation
    let task: Task<String, Error>
}

private struct RecognitionResponseBody: Decodable {
    struct Result: Decodable {
        struct Alternative: Decodable {
            let transcript: String
        }

        let alternatives: [Alternative]
    }

    let results: [Result]
}

private extension NSLock {
    func withLock<T>(_ operation: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try operation()
    }
}

private final class TranscriptEventRelay: @unchecked Sendable {
    private let lock = NSLock()
    private var storedHandler: ((TranscriptEvent) -> Void)?

    var handler: ((TranscriptEvent) -> Void)? {
        get { lock.withLock { storedHandler } }
        set { lock.withLock { storedHandler = newValue } }
    }

    func send(_ event: TranscriptEvent) {
        let handler = lock.withLock { storedHandler }
        handler?(event)
    }
}

private extension Data {
    func chunks(maximumBytes: Int) -> [Data] {
        guard !isEmpty else { return [] }
        var chunks: [Data] = []
        chunks.reserveCapacity((count + maximumBytes - 1) / maximumBytes)
        var offset = startIndex
        while offset < endIndex {
            let nextOffset = index(offset, offsetBy: maximumBytes, limitedBy: endIndex) ?? endIndex
            chunks.append(self[offset..<nextOffset])
            offset = nextOffset
        }
        return chunks
    }
}
