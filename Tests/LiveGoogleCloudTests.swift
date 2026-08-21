import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Live Google Cloud verification")
struct LiveGoogleCloudTests {
    @Test("Real ADC Gemini Flash rewrites through Vertex AI")
    func rewritesWithGeminiFlash() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["VOICE_SCRIBE_LIVE_GEMINI"] == "1",
              let projectID = environment["VOICE_SCRIBE_LIVE_GCP_PROJECT"]
        else {
            return
        }
        let client = GeminiFlashRewriteClient(
            configuration: GeminiFlashConfiguration(projectID: projectID),
            tokenProvider: GCloudApplicationDefaultCredentialsTokenProvider()
        )

        let result = try await client.rewrite(
            text: "hello from Mluva on google cloud",
            modeInstructions: "Return one concise sentence.",
            protectedVocabulary: ["Mluva", "Google Cloud"]
        )

        #expect(result.contains("Mluva"))
        #expect(result.localizedCaseInsensitiveContains("Google Cloud"))
    }

    @Test("Real ADC Gemini Flash executes a command through Vertex AI")
    func executesCommandWithGeminiFlash() async throws {
        let environment = ProcessInfo.processInfo.environment
        guard environment["VOICE_SCRIBE_LIVE_GEMINI"] == "1",
              let projectID = environment["VOICE_SCRIBE_LIVE_GCP_PROJECT"]
        else {
            return
        }
        let client = GeminiFlashRewriteClient(
            configuration: GeminiFlashConfiguration(projectID: projectID),
            tokenProvider: GCloudApplicationDefaultCredentialsTokenProvider()
        )

        let result = try await client.executeCommand(TranscriptCommandRequest(
            instruction: "Rewrite this as a concise question.",
            sourceText: "The deployment is still scheduled for Tuesday."
        ))

        #expect(result.localizedCaseInsensitiveContains("Tuesday"))
        #expect(result.contains("?"))
    }

    @Test("Real ADC streaming recognition returns speech from a PCM fixture")
    func streamsPCMFixture() async throws {
        guard #available(macOS 15.0, *) else { return }
        let environment = ProcessInfo.processInfo.environment
        guard let projectID = environment["VOICE_SCRIBE_LIVE_GCP_PROJECT"],
              let fixturePath = environment["VOICE_SCRIBE_LIVE_GCP_PCM"]
        else {
            return
        }

        let audio = try Data(contentsOf: URL(fileURLWithPath: fixturePath))
        let provider = GoogleCloudTranscriptionProvider(
            configuration: GoogleCloudSpeechConfiguration(
                projectID: projectID,
                location: environment["VOICE_SCRIBE_LIVE_GCP_LOCATION"] ?? "eu",
                model: environment["VOICE_SCRIBE_LIVE_GCP_MODEL"] ?? "chirp_3",
                languageCodes: ["en-US"]
            ),
            streamingTransport: SystemGoogleStreamingRecognitionTransport()
        )

        try await provider.start()
        for offset in stride(from: 0, to: audio.count, by: 3_200) {
            provider.appendAudio(audio[offset..<min(offset + 3_200, audio.count)])
        }
        let transcript = try await provider.finish()

        #expect(transcript.lowercased().contains("google cloud"))
    }
}
