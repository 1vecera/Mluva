import Foundation

struct VocabularySuggestion: Equatable, Identifiable, Sendable {
    let id: String
    let spoken: String
    let written: String
    let bundleIdentifier: String?
    let occurrences: Int
}

struct VocabularySuggestionEngine {
    func suggestions(
        from entries: [TranscriptionEntry],
        dictionary: [DictionaryReplacement] = [],
        dismissedIDs: Set<String> = []
    ) -> [VocabularySuggestion] {
        var grouped: [String: VocabularySuggestion] = [:]

        for entry in entries {
            guard let source = entry.correctionSourceText,
                  let correction = focusedCorrection(from: source, to: entry.deliveredText)
            else {
                continue
            }
            let id = suggestionID(
                spoken: correction.spoken,
                written: correction.written,
                bundleIdentifier: entry.targetBundleIdentifier
            )
            guard !dismissedIDs.contains(id) else { continue }

            if let existing = grouped[id] {
                grouped[id] = VocabularySuggestion(
                    id: id,
                    spoken: existing.spoken,
                    written: existing.written,
                    bundleIdentifier: existing.bundleIdentifier,
                    occurrences: existing.occurrences + 1
                )
            } else {
                grouped[id] = VocabularySuggestion(
                    id: id,
                    spoken: correction.spoken,
                    written: correction.written,
                    bundleIdentifier: entry.targetBundleIdentifier,
                    occurrences: 1
                )
            }
        }

        return grouped.values
            .filter { suggestion in
                !dictionary.contains { replacement in
                    guard normalized(replacement.spoken) == normalized(suggestion.spoken)
                    else {
                        return false
                    }
                    if suggestion.bundleIdentifier == nil {
                        return replacement.bundleIdentifier == nil
                    }
                    return replacement.bundleIdentifier == nil
                        || replacement.bundleIdentifier == suggestion.bundleIdentifier
                }
            }
            .sorted {
                if $0.occurrences != $1.occurrences {
                    return $0.occurrences > $1.occurrences
                }
                return $0.spoken.localizedCaseInsensitiveCompare($1.spoken) == .orderedAscending
            }
    }

    private func focusedCorrection(
        from source: String,
        to corrected: String
    ) -> (spoken: String, written: String)? {
        let sourceWords = words(in: source)
        let correctedWords = words(in: corrected)
        guard !sourceWords.isEmpty, !correctedWords.isEmpty else { return nil }

        var prefixCount = 0
        while prefixCount < min(sourceWords.count, correctedWords.count),
              sourceWords[prefixCount].text == correctedWords[prefixCount].text {
            prefixCount += 1
        }

        var suffixCount = 0
        while suffixCount < sourceWords.count - prefixCount,
              suffixCount < correctedWords.count - prefixCount,
              sourceWords[sourceWords.count - suffixCount - 1].text
                == correctedWords[correctedWords.count - suffixCount - 1].text {
            suffixCount += 1
        }

        let sourceChanged = Array(
            sourceWords[prefixCount..<(sourceWords.count - suffixCount)]
        )
        let correctedChanged = Array(
            correctedWords[prefixCount..<(correctedWords.count - suffixCount)]
        )
        guard !sourceChanged.isEmpty,
              !correctedChanged.isEmpty,
              sourceChanged.count <= 4,
              correctedChanged.count <= 4
        else {
            return nil
        }

        let sharedWordCount = prefixCount + suffixCount
        guard sharedWordCount > 0
            || max(sourceWords.count, correctedWords.count) <= 3
        else {
            return nil
        }

        let spoken = phrase(in: source, words: sourceChanged)
        let written = phrase(in: corrected, words: correctedChanged)
        guard spoken != written,
              spoken.count <= 80,
              written.count <= 80
        else {
            return nil
        }
        return (spoken, written)
    }

    private func words(in text: String) -> [Word] {
        guard let expression = try? NSRegularExpression(pattern: #"\S+"#) else {
            return []
        }
        return expression.matches(
            in: text,
            range: NSRange(text.startIndex..., in: text)
        ).compactMap { match in
            guard let range = Range(match.range, in: text),
                  let trimmedRange = trimmedTokenRange(range, in: text)
            else {
                return nil
            }
            return Word(text: String(text[trimmedRange]), range: trimmedRange)
        }
    }

    private func trimmedTokenRange(
        _ range: Range<String.Index>,
        in text: String
    ) -> Range<String.Index>? {
        let leadingBoundary = CharacterSet(charactersIn: "\"“”‘’([{")
        let trailingBoundary = CharacterSet(charactersIn: "\"“”‘’)]},.!?;:")
        var lowerBound = range.lowerBound
        var upperBound = range.upperBound

        while lowerBound < upperBound,
              isBoundary(text[lowerBound], in: leadingBoundary) {
            lowerBound = text.index(after: lowerBound)
        }
        while lowerBound < upperBound {
            let previous = text.index(before: upperBound)
            guard isBoundary(text[previous], in: trailingBoundary) else { break }
            upperBound = previous
        }
        return lowerBound < upperBound ? lowerBound..<upperBound : nil
    }

    private func isBoundary(_ character: Character, in set: CharacterSet) -> Bool {
        character.unicodeScalars.allSatisfy(set.contains)
    }

    private func phrase(in text: String, words: [Word]) -> String {
        guard let first = words.first, let last = words.last else { return "" }
        return String(text[first.range.lowerBound..<last.range.upperBound])
    }

    private func suggestionID(
        spoken: String,
        written: String,
        bundleIdentifier: String?
    ) -> String {
        [
            bundleIdentifier ?? "",
            normalized(spoken),
            written.precomposedStringWithCanonicalMapping,
        ].joined(separator: "\u{1F}")
    }

    private func normalized(_ value: String) -> String {
        value.folding(
            options: [.caseInsensitive, .diacriticInsensitive],
            locale: Locale(identifier: "en_US_POSIX")
        )
    }

    private struct Word {
        let text: String
        let range: Range<String.Index>
    }
}
