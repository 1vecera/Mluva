import Testing
@testable import VoiceScribeMac

@Suite("Meeting insights")
struct MeetingInsightExtractorTests {
    @Test("Explicit decisions and owners become reviewable meeting insights")
    func extractsDecisionsAndActions() {
        let transcript = """
        We reviewed the launch readiness. We decided to ship on Friday. Action item: Daniel will update the release notes. Marta will confirm the support rota.
        """

        let insights = MeetingInsightExtractor().extract(from: transcript)

        #expect(insights.summary == "We reviewed the launch readiness. We decided to ship on Friday. Action item: Daniel will update the release notes.")
        #expect(insights.decisions == ["We decided to ship on Friday."])
        #expect(insights.actionItems == [
            "Action item: Daniel will update the release notes.",
            "Marta will confirm the support rota.",
        ])
    }

    @Test("Neutral discussion does not invent decisions or action items")
    func doesNotInventInsights() {
        let insights = MeetingInsightExtractor().extract(
            from: "We explored several directions. The team shared open questions."
        )

        #expect(insights.decisions.isEmpty)
        #expect(insights.actionItems.isEmpty)
    }
}
