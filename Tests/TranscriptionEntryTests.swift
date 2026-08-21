import Testing
import Foundation
@testable import VoiceScribeMac

@Suite("Transcription Entry")
struct TranscriptionEntryTests {

    @Test("Initialization sets all fields")
    func initSetsFields() {
        let entry = TranscriptionEntry(text: "Hello world", duration: 5.5)

        #expect(entry.text == "Hello world")
        #expect(entry.duration == 5.5)
    }

    @Test("Default duration is zero")
    func defaultDuration() {
        let entry = TranscriptionEntry(text: "test")
        #expect(entry.duration == 0)
    }

    @Test("Codable roundtrip preserves all fields")
    func codableRoundtrip() throws {
        let original = TranscriptionEntry(
            rawText: "codable raw",
            deliveredText: "Codable test",
            correctionSourceText: "Codable draft",
            duration: 3.14,
            provider: .googleCloud,
            language: "en-US",
            mode: .dictation,
            targetApplicationName: "Notes",
            targetBundleIdentifier: "com.apple.Notes",
            deliveryOutcome: .delivered,
            retainedAudioFilename: "retained.pcm",
            enhancementOutcome: .applied,
            cleanupProviderID: "apple-intelligence",
            cleanupModelIdentifier: "apple.foundation-model.on-device-v1",
            styleName: "Technical notes",
            styleOutcome: .applied,
            contextSources: [.application, .windowTitle],
            fallbackEvent: ProviderFallbackEvent(
                from: .apple,
                to: .googleCloud,
                reason: .providerFinalizationFailed
            ),
            timings: TranscriptionTimings(
                captureLatency: 0.08,
                recognitionLatency: 0.24,
                enhancementLatency: 0.12,
                deliveryLatency: 0.04
            )
        )

        let encoder = JSONEncoder()
        encoder.dateEncodingStrategy = .iso8601
        let data = try encoder.encode(original)

        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        let decoded = try decoder.decode(TranscriptionEntry.self, from: data)

        #expect(decoded.id == original.id)
        #expect(decoded.text == original.text)
        #expect(decoded.rawText == "codable raw")
        #expect(decoded.correctionSourceText == "Codable draft")
        #expect(decoded.provider == .googleCloud)
        #expect(decoded.language == "en-US")
        #expect(decoded.targetBundleIdentifier == "com.apple.Notes")
        #expect(decoded.deliveryOutcome == .delivered)
        #expect(decoded.retainedAudioFilename == "retained.pcm")
        #expect(decoded.cleanupProviderID == "apple-intelligence")
        #expect(decoded.cleanupModelIdentifier == "apple.foundation-model.on-device-v1")
        #expect(decoded.styleName == "Technical notes")
        #expect(decoded.styleOutcome == .applied)
        #expect(decoded.contextSources == [.application, .windowTitle])
        #expect(decoded.fallbackEvent == original.fallbackEvent)
        #expect(decoded.timings == original.timings)
        #expect(abs(decoded.duration - original.duration) < 0.001)
    }

    @Test("Legacy text-only history remains readable")
    func decodesLegacyEntry() throws {
        let id = UUID()
        let timestamp = Date(timeIntervalSince1970: 1_700_000_000)
        let legacy = """
        {
          "id": "\(id.uuidString)",
          "text": "legacy transcript",
          "timestamp": "\(ISO8601DateFormatter().string(from: timestamp))",
          "duration": 2.5
        }
        """
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601

        let entry = try decoder.decode(TranscriptionEntry.self, from: Data(legacy.utf8))

        #expect(entry.rawText == "legacy transcript")
        #expect(entry.deliveredText == "legacy transcript")
        #expect(entry.provider == .automatic)
        #expect(entry.mode == .dictation)
        #expect(entry.deliveryOutcome == .delivered)
        #expect(entry.styleName == nil)
        #expect(entry.cleanupProviderID == nil)
        #expect(entry.cleanupModelIdentifier == nil)
        #expect(entry.styleOutcome == .notRequested)
        #expect(entry.contextSources.isEmpty)
        #expect(entry.fallbackEvent == nil)
        #expect(entry.timings == .empty)
    }

    @Test("Each entry gets a unique ID")
    func uniqueIDs() {
        let a = TranscriptionEntry(text: "a")
        let b = TranscriptionEntry(text: "b")
        #expect(a.id != b.id)
    }

    @Test("Delivery state can change without losing recovery metadata")
    func updatesDeliveryState() {
        let entry = TranscriptionEntry(
            rawText: "raw",
            deliveredText: "Delivered",
            provider: .googleCloud,
            language: "en-US",
            mode: .dictation,
            targetApplicationName: "Notes",
            targetBundleIdentifier: "com.apple.Notes",
            deliveryOutcome: .pendingDelivery,
            failureMessage: "Target unavailable",
            retainedAudioFilename: "retained.pcm"
        )

        let delivered = entry.updatingDelivery(outcome: .delivered)

        #expect(delivered.id == entry.id)
        #expect(delivered.deliveryOutcome == .delivered)
        #expect(delivered.failureMessage == nil)
        #expect(delivered.targetBundleIdentifier == "com.apple.Notes")
        #expect(delivered.retainedAudioFilename == "retained.pcm")
    }

    @Test("History edits preserve the original capture identity")
    func editsHistoryEntry() {
        let entry = TranscriptionEntry(
            rawText: "raw words",
            deliveredText: "Old words",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .delivered
        )

        let renamed = entry.renamed("Release note")
        let reprocessed = renamed.reprocessed(deliveredText: "New words")

        #expect(reprocessed.id == entry.id)
        #expect(reprocessed.title == "Release note")
        #expect(reprocessed.rawText == "raw words")
        #expect(reprocessed.deliveredText == "New words")
        #expect(reprocessed.deliveryOutcome == .pendingDelivery)
    }

    @Test("Manual correction preserves its original comparison text")
    func recordsManualCorrection() {
        let entry = TranscriptionEntry(
            rawText: "use post grass",
            deliveredText: "Use post grass in production.",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            targetBundleIdentifier: "com.apple.Notes",
            deliveryOutcome: .delivered
        )

        let corrected = entry.corrected(deliveredText: "Use Postgres in production.")
        let correctedAgain = corrected.corrected(
            deliveredText: "Use PostgreSQL in production."
        )

        #expect(corrected.correctionSourceText == "Use post grass in production.")
        #expect(corrected.deliveredText == "Use Postgres in production.")
        #expect(corrected.deliveryOutcome == .pendingDelivery)
        #expect(correctedAgain.correctionSourceText == "Use post grass in production.")
        #expect(correctedAgain.deliveredText == "Use PostgreSQL in production.")
    }

    @Test("Any non-empty delivered text can be intentionally reused from history")
    func historyTextCanBeReused() {
        let delivered = TranscriptionEntry(text: "Paste me again")
        let pending = delivered.updatingDelivery(outcome: .pendingDelivery)
        let empty = TranscriptionEntry(
            rawText: "raw only",
            deliveredText: " ",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .failed
        )

        #expect(delivered.canDeliverFromHistory)
        #expect(pending.canDeliverFromHistory)
        #expect(!empty.canDeliverFromHistory)
    }

    @Test("Restoring raw text discards derived output but preserves capture identity")
    func restoresRawTranscript() {
        let entry = TranscriptionEntry(
            rawText: "raw exact words",
            deliveredText: "Polished words.",
            correctionSourceText: "Earlier polished words.",
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .delivered,
            enhancementOutcome: .applied,
            styleName: "Prose",
            styleOutcome: .applied,
            contextSources: [.application]
        )

        let restored = entry.restoringRawTranscript()

        #expect(restored.id == entry.id)
        #expect(restored.rawText == "raw exact words")
        #expect(restored.deliveredText == "raw exact words")
        #expect(restored.correctionSourceText == nil)
        #expect(restored.deliveryOutcome == .pendingDelivery)
        #expect(restored.enhancementOutcome == .notRequested)
        #expect(restored.styleOutcome == .notRequested)
        #expect(restored.contextSources.isEmpty)
    }
}
