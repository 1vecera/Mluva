import Foundation

enum TranscriptEventKind: Equatable, Sendable {
    case volatile
    case final
}

struct TranscriptEvent: Equatable, Sendable {
    let id: String
    let text: String
    let kind: TranscriptEventKind

    static func volatile(id: String, text: String) -> Self {
        Self(id: id, text: text, kind: .volatile)
    }

    static func final(id: String, text: String) -> Self {
        Self(id: id, text: text, kind: .final)
    }
}

struct TextDelivery: Equatable, Sendable {
    let id: String
    let text: String
}

enum TextDeliveryOutcome: Equatable, Sendable {
    case inserted
    case ignoredVolatile
    case ignoredDuplicate
    case ignoredEmpty
}
