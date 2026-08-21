import Foundation

struct TranscriptionLanguage: Equatable, Sendable {
    let identifier: String

    var appleLocaleIdentifier: String {
        Self.localeIdentifiers[identifier] ?? normalizedIdentifier
    }

    var googleLanguageCode: String {
        Self.localeIdentifiers[identifier] ?? normalizedIdentifier
    }

    private var normalizedIdentifier: String {
        let source = identifier == "auto" ? Locale.current.identifier : identifier
        let identifierWithoutKeywords = source
            .split(separator: "@", maxSplits: 1)
            .first
            .map(String.init)
            ?? source
        return identifierWithoutKeywords.replacingOccurrences(of: "_", with: "-")
    }

    private static let localeIdentifiers: [String: String] = [
        "en": "en-US",
        "cs": "cs-CZ",
        "es": "es-ES",
        "fr": "fr-FR",
        "de": "de-DE",
        "it": "it-IT",
        "pt": "pt-PT",
        "nl": "nl-NL",
        "ja": "ja-JP",
        "zh": "zh-CN",
        "ko": "ko-KR",
        "pl": "pl-PL",
        "ru": "ru-RU",
    ]
}
