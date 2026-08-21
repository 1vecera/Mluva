import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Google Cloud transcription provider")
struct GoogleCloudTranscriptionProviderTests {
    @Test("Google start waits until the streaming configuration is writable")
    func waitsForStreamingReadiness() async throws {
        let streamingTransport = DelayedReadyGoogleStreamingTransport(
            delay: .milliseconds(80),
            transcript: "ready stream"
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(projectID: "example-project"),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            streamingTransport: streamingTransport
        )
        let clock = ContinuousClock()
        let startedAt = clock.now

        try await provider.start()

        #expect(startedAt.duration(to: clock.now) >= .milliseconds(70))
        provider.appendAudio(Data([0x01]))
        #expect(try await provider.finish() == "ready stream")
    }

    @Test("Google streams configuration before bounded audio chunks")
    func streamsConfigurationBeforeBoundedAudio() async throws {
        let streamingTransport = RecordingGoogleStreamingTransport(
            outcomes: [.success(transcript: "bounded", events: [])]
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(
                projectID: "example-project",
                location: "eu",
                recognizer: "_",
                model: "chirp_3",
                languageCodes: ["en-US"]
            ),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            streamingTransport: streamingTransport
        )

        try await provider.start()
        provider.appendAudio(Data(repeating: 0x7F, count: 35_001))
        #expect(try await provider.finish() == "bounded")

        let requests = try #require(await streamingTransport.attempts.first)
        #expect(requests.first == .configuration(
            GoogleStreamingRecognitionConfiguration(
                recognizer: "projects/example-project/locations/eu/recognizers/_",
                model: "chirp_3",
                languageCodes: ["en-US"],
                sampleRateHertz: 16_000,
                audioChannelCount: 1,
                interimResults: true,
                automaticPunctuation: true
            )
        ))
        let audioChunks = requests.compactMap { request -> Data? in
            guard case .audio(let data) = request else { return nil }
            return data
        }
        #expect(audioChunks.map(\.count) == [15_000, 15_000, 5_001])
        #expect(audioChunks.allSatisfy { $0.count <= 15_000 })
    }

    @Test("Google forwards volatile and final streaming transcript events")
    func forwardsStreamingTranscriptEvents() async throws {
        let expectedEvents: [TranscriptEvent] = [
            .volatile(id: "segment-a", text: "deploy"),
            .final(id: "segment-a", text: "deploy Mluva"),
            .final(id: "segment-b", text: "to GCP"),
        ]
        let streamingTransport = RecordingGoogleStreamingTransport(
            outcomes: [.success(transcript: "deploy Mluva to GCP", events: expectedEvents)]
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(projectID: "example-project"),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            streamingTransport: streamingTransport
        )
        var receivedEvents: [TranscriptEvent] = []
        provider.onEvent = { receivedEvents.append($0) }

        try await provider.start()
        provider.appendAudio(Data([0x01, 0x02]))
        let transcript = try await provider.finish()

        #expect(transcript == "deploy Mluva to GCP")
        #expect(receivedEvents == expectedEvents)
    }

    @Test("Google replays retained audio after a streaming failure")
    func replaysRetainedAudioAfterStreamingFailure() async throws {
        let streamingTransport = RecordingGoogleStreamingTransport(
            outcomes: [
                .failure(.server(statusCode: 503)),
                .success(transcript: "recovered stream", events: []),
            ]
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(projectID: "example-project"),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            streamingTransport: streamingTransport
        )

        try await provider.start()
        provider.appendAudio(Data([0x01, 0x02, 0x03]))
        await #expect(throws: GoogleCloudSpeechError.server(statusCode: 503)) {
            try await provider.finish()
        }
        #expect(try await provider.retry() == "recovered stream")

        let attempts = await streamingTransport.attempts
        #expect(attempts.count == 2)
        #expect(attempts.first == attempts.last)
    }

    @Test("Google rolls a long stream with audio overlap and transcript deduplication")
    func rollsLongStreamingAudio() async throws {
        let streamingTransport = RecordingGoogleStreamingTransport(
            outcomes: [
                .success(transcript: "hello shared words", events: []),
                .success(transcript: "shared words again", events: []),
            ]
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(
                projectID: "example-project",
                streamingRolloverAudioBytes: 6,
                streamingOverlapAudioBytes: 2
            ),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            streamingTransport: streamingTransport
        )

        try await provider.start()
        provider.appendAudio(Data([0x00, 0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07]))
        let transcript = try await provider.finish()

        let attempts = await streamingTransport.attempts
        #expect(attempts.count == 2)
        #expect(audio(in: attempts[0]) == Data([0x00, 0x01, 0x02, 0x03, 0x04, 0x05]))
        #expect(audio(in: attempts[1]) == Data([0x04, 0x05, 0x06, 0x07]))
        #expect(transcript == "hello shared words again")
    }

    @Test("Google sends authenticated V2 recognition with raw PCM configuration")
    func sendsV2RecognitionRequest() async throws {
        let response = #"{"results":[{"alternatives":[{"transcript":"deploy Mluva"}]},{"alternatives":[{"transcript":"to GCP"}]}]}"#
        let transport = RecordingHTTPDataTransport(
            statusCode: 200,
            body: Data(response.utf8)
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(
                projectID: "example-project",
                location: "eu",
                recognizer: "_",
                model: "chirp_3",
                languageCodes: ["en-US"]
            ),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            transport: transport,
            makeEventID: { "gcp-segment" }
        )
        var events: [TranscriptEvent] = []
        provider.onEvent = { events.append($0) }

        try await provider.start()
        provider.appendAudio(Data([0x01, 0x02]))
        provider.appendAudio(Data([0x03, 0x04]))
        let transcript = try await provider.finish()

        let request = try #require(await transport.lastRequest)
        #expect(request.url?.absoluteString == "https://eu-speech.googleapis.com/v2/projects/example-project/locations/eu/recognizers/_:recognize")
        #expect(request.httpMethod == "POST")
        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer test-token")
        #expect(request.value(forHTTPHeaderField: "Content-Type") == "application/json")

        let body = try #require(request.httpBody)
        let payload = try #require(JSONSerialization.jsonObject(with: body) as? [String: Any])
        let config = try #require(payload["config"] as? [String: Any])
        let decoding = try #require(config["explicitDecodingConfig"] as? [String: Any])
        #expect(decoding["encoding"] as? String == "LINEAR16")
        #expect(decoding["sampleRateHertz"] as? Int == 16_000)
        #expect(decoding["audioChannelCount"] as? Int == 1)
        #expect(config["model"] as? String == "chirp_3")
        #expect(config["languageCodes"] as? [String] == ["en-US"])
        #expect(payload["content"] as? String == Data([0x01, 0x02, 0x03, 0x04]).base64EncodedString())

        #expect(transcript == "deploy Mluva to GCP")
        #expect(events == [.final(id: "gcp-segment", text: transcript)])
    }

    @Test("Google keeps buffered audio retryable after a provider failure")
    func failedRecognitionKeepsAudioForRetry() async throws {
        let transport = RecordingHTTPDataTransport(
            responses: [
                .init(statusCode: 503, body: Data(#"{"error":{"message":"temporarily unavailable"}}"#.utf8)),
                .init(statusCode: 200, body: Data(#"{"results":[{"alternatives":[{"transcript":"recovered"}]}]}"#.utf8)),
            ]
        )
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(projectID: "example-project"),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            transport: transport
        )

        try await provider.start()
        provider.appendAudio(Data([0x01, 0x02]))
        await #expect(throws: GoogleCloudSpeechError.server(statusCode: 503)) {
            try await provider.finish()
        }
        #expect(try await provider.retry() == "recovered")
        #expect(await transport.requestCount == 2)
    }

    @Test("Google rejects an empty recording before network access")
    func rejectsEmptyRecording() async throws {
        let transport = RecordingHTTPDataTransport(statusCode: 200, body: Data())
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(projectID: "example-project"),
            tokenProvider: FixedGoogleAccessTokenProvider(token: "test-token"),
            transport: transport
        )

        try await provider.start()
        await #expect(throws: GoogleCloudSpeechError.emptyAudio) {
            try await provider.finish()
        }
        #expect(await transport.requestCount == 0)
    }

    @Test("Google CLI discovery works without a shell-initialized PATH")
    func discoversGoogleCLIFromKnownLocations() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("gcloud-locator-\(UUID().uuidString)")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        let executable = directory.appendingPathComponent("gcloud")
        try Data().write(to: executable)
        try FileManager.default.setAttributes(
            [.posixPermissions: 0o755],
            ofItemAtPath: executable.path
        )

        let located = GCloudExecutableLocator(
            environmentPath: "",
            knownURLs: [executable]
        ).locate()

        #expect(located == executable)
    }
}

private actor DelayedReadyGoogleStreamingTransport: GoogleStreamingRecognitionTransport {
    let delay: Duration
    let transcript: String

    init(delay: Duration, transcript: String) {
        self.delay = delay
        self.transcript = transcript
    }

    func recognize(
        endpointHost: String,
        requests: AsyncStream<GoogleStreamingRequest>,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onEvent: @escaping @Sendable (TranscriptEvent) -> Void
    ) async throws -> String {
        var didBecomeReady = false
        for await request in requests {
            if !didBecomeReady,
               case .configuration = request {
                try await Task.sleep(for: delay)
                didBecomeReady = true
                onReady()
            }
        }
        return transcript
    }
}

private func audio(in requests: [GoogleStreamingRequest]) -> Data {
    requests.reduce(into: Data()) { audio, request in
        guard case .audio(let chunk) = request else { return }
        audio.append(chunk)
    }
}

private actor RecordingGoogleStreamingTransport: GoogleStreamingRecognitionTransport {
    enum Outcome {
        case success(transcript: String, events: [TranscriptEvent])
        case failure(GoogleCloudSpeechError)
    }

    private var outcomes: [Outcome]
    private(set) var attempts: [[GoogleStreamingRequest]] = []

    init(outcomes: [Outcome]) {
        self.outcomes = outcomes
    }

    func recognize(
        endpointHost: String,
        requests: AsyncStream<GoogleStreamingRequest>,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onEvent: @escaping @Sendable (TranscriptEvent) -> Void
    ) async throws -> String {
        var attempt: [GoogleStreamingRequest] = []
        for await request in requests {
            attempt.append(request)
            if case .configuration = request {
                onReady()
            }
        }
        attempts.append(attempt)

        switch outcomes.removeFirst() {
        case .success(let transcript, let events):
            events.forEach(onEvent)
            return transcript
        case .failure(let error):
            throw error
        }
    }
}

private struct FixedGoogleAccessTokenProvider: GoogleAccessTokenProviding {
    let token: String

    func accessToken() async throws -> String {
        token
    }
}

private actor RecordingHTTPDataTransport: HTTPDataTransport {
    struct Response {
        let statusCode: Int
        let body: Data
    }

    private var responses: [Response]
    private(set) var requests: [URLRequest] = []

    var lastRequest: URLRequest? { requests.last }
    var requestCount: Int { requests.count }

    init(statusCode: Int, body: Data) {
        responses = [Response(statusCode: statusCode, body: body)]
    }

    init(responses: [Response]) {
        self.responses = responses
    }

    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        requests.append(request)
        let response = responses.removeFirst()
        let httpResponse = HTTPURLResponse(
            url: request.url!,
            statusCode: response.statusCode,
            httpVersion: "HTTP/2",
            headerFields: nil
        )!
        return (response.body, httpResponse)
    }
}
