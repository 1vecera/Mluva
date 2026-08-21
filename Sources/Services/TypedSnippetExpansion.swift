import Foundation

struct TypedSnippetApplicationContext: Equatable, Sendable {
    let bundleIdentifier: String?
    let isSecureTextField: Bool
}

struct TypedSnippetReplacement: Equatable, Sendable {
    let characterCount: Int
    let text: String
    let trailingText: String
}

protocol TypedSnippetReplacing: AnyObject {
    func replaceTypedSnippet(_ replacement: TypedSnippetReplacement)
}

struct TypedSnippetMatcher {
    private var buffer = ""

    mutating func append(_ text: String, maximumLength: Int) {
        guard maximumLength > 0 else {
            reset()
            return
        }
        for character in text {
            guard !character.isWhitespace else {
                reset()
                continue
            }
            buffer.append(character)
        }
        let boundedLength = maximumLength + 1
        if buffer.count > boundedLength {
            buffer.removeFirst(buffer.count - boundedLength)
        }
    }

    mutating func deleteBackward() {
        if !buffer.isEmpty {
            buffer.removeLast()
        }
    }

    mutating func complete(
        trailingText: String,
        snippets: [Snippet],
        variables: @autoclosure () -> [String: String]
    ) -> TypedSnippetReplacement? {
        defer { reset() }
        let candidates = snippets.compactMap { snippet -> (Snippet, String)? in
            guard let trigger = snippet.typedTrigger, !trigger.isEmpty else { return nil }
            return (snippet, trigger)
        }
        .sorted { $0.1.count > $1.1.count }

        guard let match = candidates.first(where: { matches($0.1) }) else {
            return nil
        }
        return TypedSnippetReplacement(
            characterCount: match.1.count,
            text: SnippetExpansionRenderer().render(
                match.0.expansion,
                variables: variables()
            ),
            trailingText: trailingText
        )
    }

    mutating func reset() {
        buffer.removeAll(keepingCapacity: true)
    }

    private func matches(_ trigger: String) -> Bool {
        guard buffer.hasSuffix(trigger) else { return false }
        guard trigger.first?.isLetter == true || trigger.first?.isNumber == true else {
            return true
        }
        let start = buffer.index(buffer.endIndex, offsetBy: -trigger.count)
        guard start != buffer.startIndex else { return true }
        let preceding = buffer[buffer.index(before: start)]
        return !preceding.isLetter && !preceding.isNumber && preceding != "_"
    }
}
