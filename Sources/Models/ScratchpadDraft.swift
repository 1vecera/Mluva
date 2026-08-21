import Foundation

struct ScratchpadDraft: Codable, Equatable, Identifiable, Sendable {
    let entry: TranscriptionEntry
    var text: String
    let deliveryID: String
    var selectedStyleID: UUID?
    var appliedStyleName: String?
    var styleOutcome: TranscriptStyleOutcome

    var id: UUID { entry.id }

    init(
        entry: TranscriptionEntry,
        text: String,
        deliveryID: String? = nil,
        selectedStyleID: UUID? = nil,
        appliedStyleName: String? = nil,
        styleOutcome: TranscriptStyleOutcome = .notRequested
    ) {
        self.entry = entry
        self.text = text
        self.deliveryID = deliveryID ?? "scratchpad-\(entry.id.uuidString)"
        self.selectedStyleID = selectedStyleID
        self.appliedStyleName = appliedStyleName
        self.styleOutcome = styleOutcome
    }

    private enum CodingKeys: String, CodingKey {
        case entry
        case text
        case deliveryID
        case selectedStyleID
        case appliedStyleName
        case styleOutcome
    }

    init(from decoder: any Decoder) throws {
        let container = try decoder.container(keyedBy: CodingKeys.self)
        entry = try container.decode(TranscriptionEntry.self, forKey: .entry)
        text = try container.decode(String.self, forKey: .text)
        deliveryID = try container.decode(String.self, forKey: .deliveryID)
        selectedStyleID = try container.decodeIfPresent(UUID.self, forKey: .selectedStyleID)
        appliedStyleName = try container.decodeIfPresent(String.self, forKey: .appliedStyleName)
        styleOutcome = try container.decodeIfPresent(
            TranscriptStyleOutcome.self,
            forKey: .styleOutcome
        ) ?? .notRequested
    }

    func encode(to encoder: any Encoder) throws {
        var container = encoder.container(keyedBy: CodingKeys.self)
        try container.encode(entry, forKey: .entry)
        try container.encode(text, forKey: .text)
        try container.encode(deliveryID, forKey: .deliveryID)
        try container.encodeIfPresent(selectedStyleID, forKey: .selectedStyleID)
        try container.encodeIfPresent(appliedStyleName, forKey: .appliedStyleName)
        try container.encode(styleOutcome, forKey: .styleOutcome)
    }
}

enum ScratchpadDraftDestination: Sendable {
    case clipboard
    case originalApplication
}

enum ScratchpadDraftError: Error, Equatable, LocalizedError {
    case empty
    case clipboardUnavailable
    case originalApplicationUnavailable
    case noStyleSelected
    case styleUnavailable
    case styleRejected

    var errorDescription: String? {
        switch self {
        case .empty:
            "The scratchpad is empty."
        case .clipboardUnavailable:
            "Mluva could not copy the scratchpad. The draft is still available."
        case .originalApplicationUnavailable:
            "The original application is unavailable. The scratchpad is still available."
        case .noStyleSelected:
            "Choose a saved style before applying it."
        case .styleUnavailable:
            "Apple Intelligence could not apply this style. The draft is unchanged."
        case .styleRejected:
            "The style changed protected facts or terms. The draft is unchanged."
        }
    }
}
