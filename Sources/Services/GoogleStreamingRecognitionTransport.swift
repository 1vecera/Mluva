import Foundation

struct GoogleStreamingRecognitionConfiguration: Equatable, Sendable {
    let recognizer: String
    let model: String
    let languageCodes: [String]
    let sampleRateHertz: Int
    let audioChannelCount: Int
    let interimResults: Bool
    let automaticPunctuation: Bool
}

enum GoogleStreamingRequest: Equatable, Sendable {
    case configuration(GoogleStreamingRecognitionConfiguration)
    case audio(Data)
}

protocol GoogleStreamingRecognitionTransport: Sendable {
    func recognize(
        endpointHost: String,
        requests: AsyncStream<GoogleStreamingRequest>,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onEvent: @escaping @Sendable (TranscriptEvent) -> Void
    ) async throws -> String
}

#if canImport(Network)
import GRPCCore
import GRPCNIOTransportHTTP2TransportServices

@available(macOS 15.0, *)
struct SystemGoogleStreamingRecognitionTransport: GoogleStreamingRecognitionTransport {
    private let makeEventID: @Sendable () -> String

    init(makeEventID: @escaping @Sendable () -> String = { UUID().uuidString }) {
        self.makeEventID = makeEventID
    }

    func recognize(
        endpointHost: String,
        requests: AsyncStream<GoogleStreamingRequest>,
        accessToken: String,
        onReady: @escaping @Sendable () -> Void,
        onEvent: @escaping @Sendable (TranscriptEvent) -> Void
    ) async throws -> String {
        var metadata = Metadata()
        metadata.addString("Bearer \(accessToken)", forKey: "authorization")

        return try await withGRPCClient(
            transport: try .http2NIOTS(
                target: .dns(host: endpointHost, port: 443),
                transportSecurity: .tls
            )
        ) { client in
            let speech = GCPSpeechV2Speech.Client(wrapping: client)
            return try await speech.streamingRecognize(
                metadata: metadata,
                requestProducer: { writer in
                    var isReady = false
                    for await request in requests {
                        try await writer.write(Self.protobufRequest(for: request))
                        if !isReady,
                           case .configuration = request {
                            isReady = true
                            onReady()
                        }
                    }
                },
                onResponse: { response in
                    var finalSegments: [String] = []
                    var activeEventID = makeEventID()

                    for try await message in response.messages {
                        for result in message.results {
                            guard let transcript = result.alternatives.first?.transcript
                                .trimmingCharacters(in: .whitespacesAndNewlines),
                                !transcript.isEmpty
                            else {
                                continue
                            }

                            if result.isFinal {
                                onEvent(.final(id: activeEventID, text: transcript))
                                finalSegments.append(transcript)
                                activeEventID = makeEventID()
                            } else {
                                onEvent(.volatile(id: activeEventID, text: transcript))
                            }
                        }
                    }

                    let transcript = finalSegments.joined(separator: " ")
                    guard !transcript.isEmpty else {
                        throw GoogleCloudSpeechError.invalidResponse
                    }
                    return transcript
                }
            )
        }
    }

    private static func protobufRequest(
        for request: GoogleStreamingRequest
    ) -> GCPSpeechV2StreamingRecognizeRequest {
        switch request {
        case .configuration(let configuration):
            var decoding = GCPSpeechV2ExplicitDecodingConfig()
            decoding.encoding = .linear16
            decoding.sampleRateHertz = Int32(configuration.sampleRateHertz)
            decoding.audioChannelCount = Int32(configuration.audioChannelCount)

            var features = GCPSpeechV2RecognitionFeatures()
            features.enableAutomaticPunctuation = configuration.automaticPunctuation

            var recognition = GCPSpeechV2RecognitionConfig()
            recognition.explicitDecodingConfig = decoding
            recognition.features = features
            recognition.model = configuration.model
            recognition.languageCodes = configuration.languageCodes

            var streamingFeatures = GCPSpeechV2StreamingRecognitionFeatures()
            streamingFeatures.interimResults = configuration.interimResults

            var streaming = GCPSpeechV2StreamingRecognitionConfig()
            streaming.config = recognition
            streaming.streamingFeatures = streamingFeatures

            var request = GCPSpeechV2StreamingRecognizeRequest()
            request.recognizer = configuration.recognizer
            request.streamingConfig = streaming
            return request

        case .audio(let audio):
            var request = GCPSpeechV2StreamingRecognizeRequest()
            request.audio = audio
            return request
        }
    }
}
#endif
