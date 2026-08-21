import Foundation

struct MeetingInsightExtractor {
    private let decisionMarkers = [
        "we decided",
        "we agreed",
        "agreed to",
        "the decision",
        "decision:",
    ]
    private let actionMarkers = [
        "action item",
        "follow up",
        "follow-up",
        "todo",
        "to-do",
    ]
    private let namedOwnerPattern = try! NSRegularExpression(
        pattern: #"(?i)(?<![\p{L}\p{N}_])(?!we\b)[\p{L}][\p{L}'’-]*\s+will\b"#
    )

    func extract(from transcript: String) -> MeetingInsights {
        let sentences = sentenceSegments(transcript)
        let decisions = sentences.filter { sentence in
            let normalized = sentence.lowercased()
            return decisionMarkers.contains { normalized.contains($0) }
        }
        let actionItems = sentences.filter { sentence in
            let normalized = sentence.lowercased()
            if actionMarkers.contains(where: normalized.contains) {
                return true
            }
            let range = NSRange(sentence.startIndex..., in: sentence)
            return namedOwnerPattern.firstMatch(in: sentence, range: range) != nil
        }
        return MeetingInsights(
            summary: sentences.prefix(3).joined(separator: " "),
            decisions: decisions,
            actionItems: actionItems
        )
    }

    private func sentenceSegments(_ transcript: String) -> [String] {
        let normalized = transcript.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !normalized.isEmpty else { return [] }
        var sentences: [String] = []
        normalized.enumerateSubstrings(
            in: normalized.startIndex..<normalized.endIndex,
            options: [.bySentences]
        ) { substring, _, _, _ in
            guard let sentence = substring?
                .trimmingCharacters(in: .whitespacesAndNewlines),
                !sentence.isEmpty
            else {
                return
            }
            sentences.append(sentence)
        }
        return sentences.isEmpty ? [normalized] : sentences
    }
}
