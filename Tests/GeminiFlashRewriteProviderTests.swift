import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Gemini Flash rewrite provider")
struct GeminiFlashRewriteProviderTests {
    @Test("Gemini 3.6 Flash uses Vertex AI with transcript-only rewrite fields")
    func sendsVertexRewriteRequest() async throws {
        let response = #"{"candidates":[{"content":{"parts":[{"text":"Deploy Mluva today."}]},"finishReason":"STOP"}]}"#
        let transport = GeminiRecordingTransport(
            statusCode: 200,
            body: Data(response.utf8)
        )
        let client = GeminiFlashRewriteClient(
            configuration: GeminiFlashConfiguration(projectID: "example-project"),
            tokenProvider: GeminiFixedTokenProvider(token: "test-token"),
            transport: transport
        )

        let result = try await client.rewrite(
            text: "deploy Mluva today",
            modeInstructions: "Rewrite as a Google Chat message.",
            protectedVocabulary: ["Mluva"]
        )

        #expect(result == "Deploy Mluva today.")
        let request = try #require(await transport.lastRequest)
        #expect(request.url?.absoluteString == "https://aiplatform.googleapis.com/v1/projects/example-project/locations/global/publishers/google/models/gemini-3.6-flash:generateContent")
        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer test-token")
        let body = try #require(request.httpBody)
        let bodyText = try #require(String(data: body, encoding: .utf8))
        #expect(bodyText.contains("deploy Mluva today"))
        #expect(bodyText.contains("Rewrite as a Google Chat message."))
        #expect(bodyText.contains("Mluva"))
        #expect(!bodyText.contains("application_context"))
    }

    @Test("Gemini command sends only the spoken instruction and selected text")
    func sendsBoundedCommandRequest() async throws {
        let response = #"{"candidates":[{"content":{"parts":[{"text":"Could you send the report today?"}]},"finishReason":"STOP"}]}"#
        let transport = GeminiRecordingTransport(
            statusCode: 200,
            body: Data(response.utf8)
        )
        let client = GeminiFlashRewriteClient(
            configuration: GeminiFlashConfiguration(projectID: "example-project"),
            tokenProvider: GeminiFixedTokenProvider(token: "test-token"),
            transport: transport
        )

        let result = try await client.executeCommand(TranscriptCommandRequest(
            instruction: "Make this friendlier",
            sourceText: "Send the report today.",
            context: TranscriptContext(
                applicationName: "Private Mail",
                windowTitle: "Confidential inbox",
                nearbyText: "Do not disclose this nearby text"
            )
        ))

        #expect(result == "Could you send the report today?")
        let request = try #require(await transport.lastRequest)
        #expect(request.url?.absoluteString == "https://aiplatform.googleapis.com/v1/projects/example-project/locations/global/publishers/google/models/gemini-3.6-flash:generateContent")
        let body = try #require(request.httpBody)
        let bodyText = try #require(String(data: body, encoding: .utf8))
        #expect(bodyText.contains("Make this friendlier"))
        #expect(bodyText.contains("Send the report today."))
        #expect(!bodyText.contains("Private Mail"))
        #expect(!bodyText.contains("Confidential inbox"))
        #expect(!bodyText.contains("Do not disclose this nearby text"))
    }

    @Test("Command uses Gemini first and calls Apple only after failure")
    func usesGeminiBeforeAppleFallback() async throws {
        let geminiPrimary = GeminiCommandProbe(result: "Cloud result")
        let unusedAppleFallback = GeminiCommandProbe(result: "Local result")
        let geminiFirst = PrimaryThenFallbackTranscriptCommander(
            primary: geminiPrimary,
            fallback: unusedAppleFallback
        )
        let request = TranscriptCommandRequest(
            instruction: "Shorten this",
            sourceText: "A long sentence"
        )

        #expect(try await geminiFirst.execute(request) == "Cloud result")
        #expect(await geminiPrimary.callCount == 1)
        #expect(await unusedAppleFallback.callCount == 0)

        let failedGemini = GeminiCommandProbe(result: nil)
        let usedAppleFallback = GeminiCommandProbe(result: "Local result")
        let fallbackChain = PrimaryThenFallbackTranscriptCommander(
            primary: failedGemini,
            fallback: usedAppleFallback
        )

        #expect(try await fallbackChain.execute(request) == "Local result")
        #expect(await failedGemini.callCount == 1)
        #expect(await usedAppleFallback.callCount == 1)
    }

    @Test("Gemini cleanup exposes the exact current model identifier")
    func exposesStableProviderIdentity() {
        let provider = GeminiFlashCleanupProvider(
            rewriter: GeminiFixedRewriter(result: "Ready")
        )

        #expect(provider.descriptor.id == "google-gemini-flash")
        #expect(provider.descriptor.modelIdentifier == "gemini-3.6-flash")
        #expect(provider.descriptor.capabilities.supportedContextSources.isEmpty)
        #expect(!provider.descriptor.capabilities.supportsIncognito)
    }

    @Test("Gemini cleanup maps quota failures to raw fallback")
    func mapsQuotaFailure() async {
        let provider = GeminiFlashCleanupProvider(
            rewriter: GeminiFailingRewriter(error: .quota)
        )
        let request = CleanupRequest(
            sessionID: "session",
            segmentID: "segment",
            segmentSequence: 0,
            rawText: "raw words",
            preparedText: "raw words",
            context: .empty,
            protectedVocabulary: [],
            voiceProfile: .faithful,
            maximumResponseCharacters: 1_000
        )

        #expect(await provider.cleanup(request) == .failure(.quota))
    }
}

private actor GeminiRecordingTransport: HTTPDataTransport {
    private let statusCode: Int
    private let body: Data
    private(set) var lastRequest: URLRequest?

    init(statusCode: Int, body: Data) {
        self.statusCode = statusCode
        self.body = body
    }

    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        lastRequest = request
        return (
            body,
            HTTPURLResponse(
                url: request.url!,
                statusCode: statusCode,
                httpVersion: "HTTP/2",
                headerFields: nil
            )!
        )
    }
}

private struct GeminiFixedTokenProvider: GoogleAccessTokenProviding {
    let token: String

    func accessToken() async throws -> String { token }
}

private struct GeminiFixedRewriter: GeminiFlashRewriting {
    let result: String

    func rewrite(
        text: String,
        modeInstructions: String?,
        protectedVocabulary: [String]
    ) async throws -> String {
        result
    }
}

private struct GeminiFailingRewriter: GeminiFlashRewriting {
    let error: GeminiFlashRewriteError

    func rewrite(
        text: String,
        modeInstructions: String?,
        protectedVocabulary: [String]
    ) async throws -> String {
        throw error
    }
}

private actor GeminiCommandProbe: TranscriptCommanding {
    private let result: String?
    private(set) var callCount = 0

    init(result: String?) {
        self.result = result
    }

    func execute(_ request: TranscriptCommandRequest) async throws -> String {
        callCount += 1
        guard let result else { throw GeminiCommandProbeError.failed }
        return result
    }
}

private enum GeminiCommandProbeError: Error {
    case failed
}
