import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Faithful transcript processing")
struct TranscriptProcessorTests {
    @Test("Processing preserves the immutable raw transcript")
    func preservesRawTranscript() {
        let processor = TranscriptProcessor()
        let result = processor.process(
            "I um use post grass new paragraph daily",
            configuration: TranscriptProcessingConfiguration(
                removeFillers: true,
                dictionary: [DictionaryReplacement(spoken: "post grass", written: "Postgres")]
            )
        )

        #expect(result.raw == "I um use post grass new paragraph daily")
        #expect(result.text == "I use Postgres\n\ndaily")
    }

    @Test("Scratch that discards speech before the latest correction")
    func scratchThatUsesLatestThought() {
        let result = TranscriptProcessor().process(
            "Ship on Thursday scratch that ship on Friday",
            configuration: .faithful
        )

        #expect(result.text == "ship on Friday")
    }

    @Test("Dictionary matching is case insensitive and uses whole phrases")
    func dictionaryUsesWholePhrases() {
        let configuration = TranscriptProcessingConfiguration(
            dictionary: [
                DictionaryReplacement(spoken: "project mluva", written: "Mluva"),
                DictionaryReplacement(spoken: "g c p", written: "GCP"),
            ]
        )

        let result = TranscriptProcessor().process(
            "PROJECT MLUVA uses g c p but project mluvas is untouched",
            configuration: configuration
        )

        #expect(result.text == "Mluva uses GCP but project mluvas is untouched")
    }

    @Test("Dictionary can preserve the matched capitalization pattern")
    func dictionaryPreservesCasePattern() {
        let replacement = DictionaryReplacement(
            spoken: "example value",
            written: "replacement text",
            caseBehavior: .matchSpoken
        )

        let result = TranscriptProcessor().process(
            "EXAMPLE VALUE, Example value, and example value",
            configuration: TranscriptProcessingConfiguration(dictionary: [replacement])
        )

        #expect(result.text == "REPLACEMENT TEXT, Replacement text, and replacement text")
    }

    @Test("Explicit spoken snippet trigger expands exactly")
    func expandsExplicitSnippet() {
        let configuration = TranscriptProcessingConfiguration(
            snippets: [Snippet(trigger: "email signoff", expansion: "Best,\nDaniel")]
        )

        let result = TranscriptProcessor().process(
            "Thanks for the review snippet email signoff",
            configuration: configuration
        )

        #expect(result.text == "Thanks for the review Best,\nDaniel")
    }

    @Test("Snippet variables expand deterministically and unknown variables remain visible")
    func expandsSnippetVariables() {
        let configuration = TranscriptProcessingConfiguration(
            snippets: [Snippet(
                trigger: "daily stamp",
                expansion: "{{date}} at {{time}} {{unknown}} costs $5"
            )],
            snippetVariables: [
                "date": "July 31, 2026",
                "time": "14:30",
            ]
        )

        let result = TranscriptProcessor().process(
            "snippet daily stamp",
            configuration: configuration
        )

        #expect(result.text == "July 31, 2026 at 14:30 {{unknown}} costs $5")
    }

    @Test("Snippet variables use the configured local calendar")
    func resolvesLocalSnippetVariables() {
        let resolver = SnippetVariableResolver(
            now: { Date(timeIntervalSince1970: 1_785_508_200) },
            locale: Locale(identifier: "en_US_POSIX"),
            timeZone: TimeZone(secondsFromGMT: 0)!
        )

        let values = resolver.values()

        #expect(values["date"] == "Jul 31, 2026")
        #expect(values["time"]?.contains("2:30") == true)
        #expect(values["time"]?.contains("PM") == true)
        #expect(values["weekday"] == "Friday")
    }

    @Test("Unknown snippet trigger remains literal")
    func leavesUnknownSnippetLiteral() {
        let result = TranscriptProcessor().process(
            "snippet customer reply",
            configuration: .faithful
        )

        #expect(result.text == "snippet customer reply")
    }

    @Test("Protected technical tokens reveal unsafe enhancement")
    func detectsChangedProtectedTokens() {
        let validator = TranscriptIntegrityValidator()
        let violations = validator.violations(
            source: "Deploy PostgreSQL 17 to eu-west1 with --dry-run at 12.5%.",
            candidate: "Deploy PostgreSQL to eu-west with dry run at 12%.",
            protectedVocabulary: ["PostgreSQL"]
        )

        #expect(violations.map(\.token) == ["--dry-run", "12.5%", "17", "eu-west1"])
    }

    @Test("Integrity validation protects URLs paths identifiers and negation")
    func protectsMeaningCriticalTokens() {
        let violations = TranscriptIntegrityValidator().violations(
            source: "Do not POST https://example.com/api to /srv/voice_scribe with requestID.",
            candidate: "POST https://example.org to /srv with requestId."
        )

        #expect(violations.map(\.token) == [
            "/srv/voice_scribe",
            "Do not",
            "https://example.com/api",
            "requestID",
        ])
    }

    @Test("Faithful enhancement accepts safe cleanup")
    func acceptsSafeEnhancement() async {
        let enhancer = FaithfulTranscriptEnhancer(
            backend: FixedEnhancementBackend(
                candidate: "Deploy PostgreSQL 17 with --dry-run."
            )
        )

        let result = await enhancer.enhance(TranscriptEnhancementRequest(
            text: "deploy PostgreSQL 17 with --dry-run",
            targetApplicationName: "Code",
            protectedVocabulary: ["PostgreSQL"]
        ))

        #expect(result.text == "Deploy PostgreSQL 17 with --dry-run.")
        #expect(result.outcome == .applied)
    }

    @Test("Faithful enhancement rejects a fluent meaning change")
    func rejectsUnsafeEnhancement() async {
        let source = "Do not deploy PostgreSQL 17 with --dry-run."
        let enhancer = FaithfulTranscriptEnhancer(
            backend: FixedEnhancementBackend(
                candidate: "Deploy PostgreSQL with force enabled."
            )
        )

        let result = await enhancer.enhance(TranscriptEnhancementRequest(
            text: source,
            protectedVocabulary: ["PostgreSQL"]
        ))

        #expect(result.text == source)
        #expect(result.outcome == .rejectedUnsafe)
        #expect(result.violations.map(\.token) == ["--dry-run", "17", "Do not"])
    }

    @Test("Saved styles preserve protected facts while changing presentation")
    func appliesSafeSavedStyle() async throws {
        let style = SavedStyle(
            name: "Technical notes",
            instructions: "Use compact technical notes."
        )
        let styler = FaithfulTranscriptStyler(
            backend: FixedStyleBackend(
                candidate: "Deploy PostgreSQL 17\n- Keep --dry-run enabled"
            )
        )

        let result = await styler.apply(TranscriptStyleRequest(
            text: "deploy PostgreSQL 17 and keep --dry-run enabled",
            style: style,
            targetApplicationName: "Code",
            protectedVocabulary: ["PostgreSQL"]
        ))

        #expect(result.text == "Deploy PostgreSQL 17\n- Keep --dry-run enabled")
        #expect(result.outcome == .applied)
    }

    @Test("Saved styles reject changes to protected facts")
    func rejectsUnsafeSavedStyle() async {
        let source = "Do not deploy PostgreSQL 17 with --dry-run."
        let styler = FaithfulTranscriptStyler(
            backend: FixedStyleBackend(candidate: "Deploy PostgreSQL 18 now.")
        )

        let result = await styler.apply(TranscriptStyleRequest(
            text: source,
            style: SavedStyle(name: "Prose", instructions: "Polish the prose."),
            protectedVocabulary: ["PostgreSQL"]
        ))

        #expect(result.text == source)
        #expect(result.outcome == .rejectedUnsafe)
        #expect(result.violations.map(\.token) == ["--dry-run", "17", "Do not"])
    }
}

private struct FixedEnhancementBackend: TranscriptEnhancementBackend {
    let candidate: String

    func enhance(_ request: TranscriptEnhancementRequest) async throws -> String {
        candidate
    }
}

private struct FixedStyleBackend: TranscriptStyleBackend {
    let candidate: String

    func apply(_ request: TranscriptStyleRequest) async throws -> String {
        candidate
    }
}
