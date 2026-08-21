import Foundation

enum TranscriptContextSource: String, Codable, Equatable, Hashable, Sendable {
    case application
    case windowTitle
    case selectedText
    case nearbyText

    var displayName: String {
        switch self {
        case .application: "application"
        case .windowTitle: "window title"
        case .selectedText: "selected text"
        case .nearbyText: "nearby text"
        }
    }
}

struct TranscriptContext: Codable, Equatable, Sendable {
    let applicationName: String?
    let windowTitle: String?
    let selectedText: String?
    let nearbyText: String?

    static let empty = TranscriptContext()

    init(
        applicationName: String? = nil,
        windowTitle: String? = nil,
        selectedText: String? = nil,
        nearbyText: String? = nil
    ) {
        self.applicationName = Self.bounded(applicationName, limit: 200)
        self.windowTitle = Self.bounded(windowTitle, limit: 400)
        self.selectedText = Self.bounded(selectedText, limit: 2_000)
        self.nearbyText = Self.bounded(nearbyText, limit: 4_000)
    }

    var sources: [TranscriptContextSource] {
        var result: [TranscriptContextSource] = []
        if applicationName != nil { result.append(.application) }
        if windowTitle != nil { result.append(.windowTitle) }
        if selectedText != nil { result.append(.selectedText) }
        if nearbyText != nil { result.append(.nearbyText) }
        return result
    }

    var modelPrompt: String {
        let values: [(String, String?)] = [
            ("Application", applicationName),
            ("Window title", windowTitle),
            ("Selected text", selectedText),
            ("Nearby text", nearbyText),
        ]
        let lines = values.compactMap { label, value in
            value.map { "\(label): \($0)" }
        }
        return lines.isEmpty ? "No application context is available." : lines.joined(separator: "\n")
    }

    private static func bounded(_ value: String?, limit: Int) -> String? {
        guard let value else { return nil }
        let trimmed = value.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }
        return String(trimmed.prefix(limit))
    }
}
