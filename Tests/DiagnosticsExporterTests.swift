import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Diagnostics exporter")
struct DiagnosticsExporterTests {
    @Test("Diagnostics export includes timings but excludes private content")
    func exportsRedactedDiagnostics() throws {
        let settings = AppSettings(
            defaults: UserDefaults(suiteName: "diagnostics-\(UUID().uuidString)")!
        )
        settings.language = "en-US"
        settings.providerPreference = .googleCloud
        settings.cloudRecognitionAllowed = true
        settings.googleCloudProjectID = "secret-project-id"
        let entry = TranscriptionEntry(
            rawText: "secret raw transcript",
            deliveredText: "secret delivered transcript",
            duration: 2.4,
            provider: .googleCloud,
            language: "en-US",
            mode: .dictation,
            targetApplicationName: "Secret Application",
            targetBundleIdentifier: "com.example.secret",
            deliveryOutcome: .failed,
            failureMessage: "secret provider message",
            retainedAudioFilename: "secret-audio.pcm",
            fallbackEvent: ProviderFallbackEvent(
                from: .apple,
                to: .googleCloud,
                reason: .providerFinalizationFailed
            ),
            timings: TranscriptionTimings(
                captureLatency: 0.08,
                recognitionLatency: 0.24,
                enhancementLatency: 0.12,
                deliveryLatency: nil
            )
        )

        let data = try TranscriptionDiagnosticsExporter().export(
            settings: settings,
            entries: [entry]
        )
        let json = try #require(String(data: data, encoding: .utf8))

        #expect(json.contains("captureLatency"))
        #expect(json.contains("recognitionOrProcessing"))
        #expect(json.contains("googleCloudConfigured"))
        #expect(json.contains("providerFinalizationFailed"))
        #expect(!json.contains("secret raw transcript"))
        #expect(!json.contains("secret delivered transcript"))
        #expect(!json.contains("secret provider message"))
        #expect(!json.contains("secret-audio.pcm"))
        #expect(!json.contains("secret-project-id"))
        #expect(!json.contains("Secret Application"))
        #expect(!json.contains("com.example.secret"))
    }
}
