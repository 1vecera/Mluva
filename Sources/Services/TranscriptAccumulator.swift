import Foundation

struct TranscriptAccumulator {
    private var committedText = ""
    private var volatileTextByID: [String: String] = [:]
    private var latestVolatileID: String?
    private var committedEventIDs: Set<String> = []

    var finalText: String {
        committedText.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    var displayText: String {
        let volatileText = latestVolatileID.flatMap { volatileTextByID[$0] } ?? ""
        return Self.join(committedText, volatileText)
    }

    mutating func ingest(_ event: TranscriptEvent) {
        switch event.kind {
        case .volatile:
            guard !committedEventIDs.contains(event.id) else { return }
            volatileTextByID[event.id] = event.text
            latestVolatileID = event.id

        case .final:
            guard committedEventIDs.insert(event.id).inserted else { return }
            volatileTextByID.removeValue(forKey: event.id)
            if latestVolatileID == event.id {
                latestVolatileID = nil
            }
            committedText = Self.mergeRollover(committedText, event.text)
        }
    }

    private static func join(_ leading: String, _ trailing: String) -> String {
        let left = leading.trimmingCharacters(in: .whitespacesAndNewlines)
        let right = trailing.trimmingCharacters(in: .whitespacesAndNewlines)
        if left.isEmpty { return right }
        if right.isEmpty { return left }
        return "\(left) \(right)"
    }

    private static func mergeRollover(_ existing: String, _ incoming: String) -> String {
        let existingWords = existing.split(whereSeparator: \.isWhitespace).map(String.init)
        let incomingWords = incoming.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !existingWords.isEmpty else {
            return incomingWords.joined(separator: " ")
        }
        guard !incomingWords.isEmpty else {
            return existingWords.joined(separator: " ")
        }

        let overlap = rolloverOverlap(existingWords, incomingWords)

        return (existingWords + incomingWords.dropFirst(overlap)).joined(separator: " ")
    }

    static func mergeFinalSegments(_ segments: [String]) -> String {
        segments.reduce("") { mergeRollover($0, $1) }
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    static func nonOverlappingSuffix(existing: String, incoming: String) -> String {
        let existingWords = existing.split(whereSeparator: \.isWhitespace).map(String.init)
        let incomingWords = incoming.split(whereSeparator: \.isWhitespace).map(String.init)
        guard !incomingWords.isEmpty else { return "" }
        let overlap = rolloverOverlap(existingWords, incomingWords)
        return incomingWords.dropFirst(overlap).joined(separator: " ")
    }

    private static func rolloverOverlap(
        _ existingWords: [String],
        _ incomingWords: [String]
    ) -> Int {
        let maximumOverlap = min(existingWords.count, incomingWords.count)
        guard maximumOverlap > 0 else { return 0 }
        for candidate in stride(from: maximumOverlap, through: 1, by: -1) {
            let existingSuffix = existingWords.suffix(candidate).map(normalizedWord)
            let incomingPrefix = incomingWords.prefix(candidate).map(normalizedWord)
            if existingSuffix == incomingPrefix {
                return candidate
            }
        }
        return 0
    }

    private static func normalizedWord(_ word: String) -> String {
        word.trimmingCharacters(in: .punctuationCharacters).lowercased()
    }
}
