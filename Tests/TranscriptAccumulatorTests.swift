import Testing
@testable import VoiceScribeMac

@Suite("Streaming transcript accumulation")
struct TranscriptAccumulatorTests {
    @Test("Volatile updates replace rather than append")
    func volatileUpdatesReplace() {
        var accumulator = TranscriptAccumulator()

        accumulator.ingest(.volatile(id: "stream-a", text: "hello"))
        accumulator.ingest(.volatile(id: "stream-a", text: "hello world"))

        #expect(accumulator.displayText == "hello world")
        #expect(accumulator.finalText.isEmpty)
    }

    @Test("Final event moves text from volatile to committed")
    func finalEventCommits() {
        var accumulator = TranscriptAccumulator()

        accumulator.ingest(.volatile(id: "segment-a", text: "hello wor"))
        accumulator.ingest(.final(id: "segment-a", text: "hello world"))

        #expect(accumulator.displayText == "hello world")
        #expect(accumulator.finalText == "hello world")
    }

    @Test("Duplicate final event is idempotent")
    func duplicateFinalIsIgnored() {
        var accumulator = TranscriptAccumulator()
        let event = TranscriptEvent.final(id: "segment-a", text: "hello world")

        accumulator.ingest(event)
        accumulator.ingest(event)

        #expect(accumulator.finalText == "hello world")
    }

    @Test("Rollover removes the longest repeated word boundary")
    func rolloverDeduplicatesOverlap() {
        var accumulator = TranscriptAccumulator()

        accumulator.ingest(.final(id: "stream-a", text: "deploy Mluva to production"))
        accumulator.ingest(.final(id: "stream-b", text: "to production after the checks pass"))

        #expect(accumulator.finalText == "deploy Mluva to production after the checks pass")
    }

    @Test("Provider finish exposes only its non-overlapping suffix")
    func providerFinishSuffix() {
        #expect(TranscriptAccumulator.nonOverlappingSuffix(
            existing: "hello complete",
            incoming: "hello complete world"
        ) == "world")
        #expect(TranscriptAccumulator.nonOverlappingSuffix(
            existing: "hello complete",
            incoming: "hello complete"
        ).isEmpty)
    }
}
