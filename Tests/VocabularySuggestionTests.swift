import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Vocabulary suggestions")
struct VocabularySuggestionTests {
    @Test("A focused manual correction suggests only the changed phrase")
    func suggestsMinimalCorrection() throws {
        let entry = makeEntry(
            deliveredText: "Use post grass in production.",
            bundleIdentifier: "com.microsoft.VSCode"
        ).corrected(deliveredText: "Use Postgres in production.")

        let suggestion = try #require(
            VocabularySuggestionEngine().suggestions(from: [entry]).first
        )

        #expect(suggestion.spoken == "post grass")
        #expect(suggestion.written == "Postgres")
        #expect(suggestion.bundleIdentifier == "com.microsoft.VSCode")
        #expect(suggestion.occurrences == 1)
    }

    @Test("Repeated corrections combine while broad rewrites stay out")
    func combinesRepeatedFocusedCorrections() {
        let first = makeEntry(deliveredText: "Ship through cube control.")
            .corrected(deliveredText: "Ship through kubectl.")
        let second = makeEntry(deliveredText: "Run cube control now.")
            .corrected(deliveredText: "Run kubectl now.")
        let rewrite = makeEntry(
            deliveredText: "This is a complete sentence that needs work."
        ).corrected(deliveredText: "Rewrite the entire idea in a different way.")

        let suggestions = VocabularySuggestionEngine().suggestions(
            from: [first, second, rewrite]
        )

        #expect(suggestions.count == 1)
        #expect(suggestions.first?.spoken == "cube control")
        #expect(suggestions.first?.written == "kubectl")
        #expect(suggestions.first?.occurrences == 2)
    }

    @Test("Existing and dismissed replacements are not suggested")
    func excludesReviewedSuggestions() throws {
        let entry = makeEntry(deliveredText: "Use post grass here.")
            .corrected(deliveredText: "Use Postgres here.")
        let engine = VocabularySuggestionEngine()
        let suggestion = try #require(engine.suggestions(from: [entry]).first)

        #expect(engine.suggestions(
            from: [entry],
            dictionary: [DictionaryReplacement(
                spoken: "post grass",
                written: "Postgres"
            )]
        ).isEmpty)
        #expect(engine.suggestions(
            from: [entry],
            dismissedIDs: [suggestion.id]
        ).isEmpty)
    }

    @Test("Technical punctuation remains part of the suggested term")
    func preservesTechnicalPunctuation() throws {
        let entry = makeEntry(deliveredText: "Compile with C plus plus today.")
            .corrected(deliveredText: "Compile with C++ today.")

        let suggestion = try #require(
            VocabularySuggestionEngine().suggestions(from: [entry]).first
        )

        #expect(suggestion.spoken == "C plus plus")
        #expect(suggestion.written == "C++")
    }

    private func makeEntry(
        deliveredText: String,
        bundleIdentifier: String? = nil
    ) -> TranscriptionEntry {
        TranscriptionEntry(
            rawText: deliveredText,
            deliveredText: deliveredText,
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            targetBundleIdentifier: bundleIdentifier,
            deliveryOutcome: .delivered
        )
    }
}
