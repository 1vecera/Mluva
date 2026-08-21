import Combine
import Foundation

final class PersonalizationStore: ObservableObject {
    static let shared = PersonalizationStore()

    @Published private(set) var dictionary: [DictionaryReplacement] = []
    @Published private(set) var snippets: [Snippet] = []
    @Published private(set) var styles: [SavedStyle] = SavedStyle.builtIns
    @Published private(set) var dismissedVocabularySuggestionIDs: Set<String> = []
    @Published private(set) var persistenceError: String?

    private var defaultStyleID: UUID?
    private var applicationStyleIDs: [String: UUID] = [:]
    private var applicationModes: [String: TranscriptionMode] = [:]
    private var applicationProviderPreferences: [String: TranscriptionProviderKind] = [:]

    private struct Document: Codable {
        var dictionary: [DictionaryReplacement]
        var snippets: [Snippet]
        var styles: [SavedStyle]?
        var defaultStyleID: UUID?
        var applicationStyleIDs: [String: UUID]?
        var applicationModes: [String: TranscriptionMode]?
        var applicationProviderPreferences: [String: TranscriptionProviderKind]?
        var dismissedVocabularySuggestionIDs: [String]?
    }

    private let fileURL: URL

    init(fileURL: URL? = nil) {
        self.fileURL = fileURL ?? Self.defaultFileURL()
        load()
    }

    func saveDictionaryReplacement(
        spoken: String,
        written: String,
        bundleIdentifier: String? = nil,
        caseBehavior: DictionaryCaseBehavior = .fixed
    ) {
        let spoken = trimmed(spoken)
        let written = trimmed(written)
        let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        guard !spoken.isEmpty, !written.isEmpty else { return }

        if let index = dictionary.firstIndex(where: {
            $0.spoken.compare(spoken, options: [.caseInsensitive, .diacriticInsensitive]) == .orderedSame
                && $0.bundleIdentifier == bundleIdentifier
        }) {
            dictionary[index] = DictionaryReplacement(
                id: dictionary[index].id,
                spoken: spoken,
                written: written,
                bundleIdentifier: bundleIdentifier,
                caseBehavior: caseBehavior
            )
        } else {
            dictionary.append(DictionaryReplacement(
                spoken: spoken,
                written: written,
                bundleIdentifier: bundleIdentifier,
                caseBehavior: caseBehavior
            ))
        }
        persist()
    }

    func deleteDictionaryReplacement(id: UUID) {
        dictionary.removeAll { $0.id == id }
        persist()
    }

    func saveSnippet(
        trigger: String,
        typedTrigger: String? = nil,
        expansion: String,
        bundleIdentifier: String? = nil
    ) {
        let trigger = trimmed(trigger)
        let typedTrigger = normalizedOptionalText(typedTrigger)
        let expansion = trimmed(expansion)
        let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        guard !trigger.isEmpty, !expansion.isEmpty else { return }
        guard typedTrigger?.contains(where: \.isWhitespace) != true else {
            persistenceError = "Typed triggers cannot contain whitespace."
            return
        }

        if let index = snippets.firstIndex(where: { existing in
            guard existing.bundleIdentifier == bundleIdentifier else { return false }
            let matchesSpokenTrigger = existing.trigger.compare(
                trigger,
                options: [.caseInsensitive, .diacriticInsensitive]
            ) == .orderedSame
            let matchesTypedTrigger = typedTrigger.map {
                $0 == existing.typedTrigger
            } ?? false
            return matchesSpokenTrigger || matchesTypedTrigger
        }) {
            snippets[index] = Snippet(
                id: snippets[index].id,
                trigger: trigger,
                typedTrigger: typedTrigger,
                expansion: expansion,
                bundleIdentifier: bundleIdentifier
            )
        } else {
            snippets.append(Snippet(
                trigger: trigger,
                typedTrigger: typedTrigger,
                expansion: expansion,
                bundleIdentifier: bundleIdentifier
            ))
        }
        persist()
    }

    func deleteSnippet(id: UUID) {
        snippets.removeAll { $0.id == id }
        persist()
    }

    func dismissVocabularySuggestion(id: String) {
        guard !id.isEmpty else { return }
        dismissedVocabularySuggestionIDs.insert(id)
        persist()
    }

    @discardableResult
    func saveStyle(name: String, instructions: String) -> UUID? {
        let name = trimmed(name)
        let instructions = trimmed(instructions)
        guard !name.isEmpty, !instructions.isEmpty else { return nil }
        guard !styles.contains(where: {
            $0.isBuiltIn && normalizedPhrase($0.name) == normalizedPhrase(name)
        }) else {
            return nil
        }

        let styleID: UUID
        if let index = styles.firstIndex(where: {
            !$0.isBuiltIn && normalizedPhrase($0.name) == normalizedPhrase(name)
        }) {
            styleID = styles[index].id
            styles[index] = SavedStyle(
                id: styleID,
                name: name,
                instructions: instructions
            )
        } else {
            let style = SavedStyle(name: name, instructions: instructions)
            styleID = style.id
            styles.append(style)
        }
        persist()
        return styleID
    }

    @discardableResult
    func updateStyle(id: UUID, name: String, instructions: String) -> Bool {
        let name = trimmed(name)
        let instructions = trimmed(instructions)
        guard !name.isEmpty,
              !instructions.isEmpty,
              let index = styles.firstIndex(where: { $0.id == id && !$0.isBuiltIn }),
              !styles.contains(where: {
                  $0.id != id && normalizedPhrase($0.name) == normalizedPhrase(name)
              })
        else {
            return false
        }
        styles[index] = SavedStyle(
            id: id,
            name: name,
            instructions: instructions
        )
        persist()
        return true
    }

    func deleteStyle(id: UUID) {
        guard styles.first(where: { $0.id == id })?.isBuiltIn == false else { return }
        styles.removeAll { $0.id == id }
        if defaultStyleID == id {
            defaultStyleID = nil
        }
        applicationStyleIDs = applicationStyleIDs.filter { $0.value != id }
        persist()
    }

    func selectStyle(
        _ id: UUID?,
        for bundleIdentifier: String?,
        rememberPerApplication: Bool
    ) {
        let validatedID = id.flatMap { candidate in
            styles.contains(where: { $0.id == candidate }) ? candidate : nil
        }
        if rememberPerApplication,
           let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier) {
            applicationStyleIDs[bundleIdentifier] = validatedID
        } else {
            defaultStyleID = validatedID
        }
        persist()
    }

    func selectedStyle(
        for bundleIdentifier: String?,
        rememberPerApplication: Bool
    ) -> SavedStyle? {
        let selectedID: UUID?
        if rememberPerApplication,
           let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier) {
            selectedID = applicationStyleIDs[bundleIdentifier]
        } else {
            selectedID = defaultStyleID
        }
        return selectedID.flatMap { id in styles.first { $0.id == id } }
    }

    func style(id: UUID?) -> SavedStyle? {
        guard let id else { return nil }
        return styles.first { $0.id == id }
    }

    func selectMode(
        _ mode: TranscriptionMode,
        for bundleIdentifier: String?,
        rememberPerApplication: Bool
    ) {
        guard rememberPerApplication,
              let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        else {
            return
        }
        applicationModes[bundleIdentifier] = mode
        persist()
    }

    func selectedMode(
        for bundleIdentifier: String?,
        rememberPerApplication: Bool,
        fallback: TranscriptionMode
    ) -> TranscriptionMode {
        guard rememberPerApplication,
              let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        else {
            return fallback
        }
        return applicationModes[bundleIdentifier] ?? fallback
    }

    func selectProvider(
        _ provider: TranscriptionProviderKind,
        for bundleIdentifier: String?,
        rememberPerApplication: Bool
    ) {
        guard rememberPerApplication,
              let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        else {
            return
        }
        applicationProviderPreferences[bundleIdentifier] = provider
        persist()
    }

    func selectedProvider(
        for bundleIdentifier: String?,
        rememberPerApplication: Bool,
        fallback: TranscriptionProviderKind
    ) -> TranscriptionProviderKind {
        guard rememberPerApplication,
              let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier)
        else {
            return fallback
        }
        return applicationProviderPreferences[bundleIdentifier] ?? fallback
    }

    func processingConfiguration(
        removeFillers: Bool,
        targetBundleIdentifier: String? = nil
    ) -> TranscriptProcessingConfiguration {
        TranscriptProcessingConfiguration(
            removeFillers: removeFillers,
            dictionary: scopedDictionary(for: targetBundleIdentifier),
            snippets: scopedSnippets(for: targetBundleIdentifier),
            snippetVariables: SnippetVariableResolver().values()
        )
    }

    var recognitionContext: [String] {
        let candidates = dictionary.flatMap { [$0.spoken, $0.written] }
            + snippets.map(\.trigger)
        var seen: Set<String> = []
        return candidates.filter { candidate in
            let key = candidate.folding(
                options: [.caseInsensitive, .diacriticInsensitive],
                locale: .current
            )
            return seen.insert(key).inserted
        }
    }

    func snippets(for bundleIdentifier: String?) -> [Snippet] {
        scopedSnippets(for: bundleIdentifier)
    }

    private func load() {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        do {
            let data = try Data(contentsOf: fileURL)
            let document = try JSONDecoder().decode(Document.self, from: data)
            dictionary = document.dictionary
            snippets = document.snippets
            let customStyles = (document.styles ?? []).filter { !$0.isBuiltIn }
            styles = SavedStyle.builtIns + customStyles
            defaultStyleID = document.defaultStyleID
            applicationStyleIDs = document.applicationStyleIDs ?? [:]
            applicationModes = document.applicationModes ?? [:]
            applicationProviderPreferences = document.applicationProviderPreferences ?? [:]
            dismissedVocabularySuggestionIDs = Set(
                document.dismissedVocabularySuggestionIDs ?? []
            )
            discardMissingStyleSelections()
            persistenceError = nil
        } catch {
            persistenceError = error.localizedDescription
        }
    }

    private func persist() {
        do {
            try FileManager.default.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let encoder = JSONEncoder()
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            let data = try encoder.encode(Document(
                dictionary: dictionary,
                snippets: snippets,
                styles: styles.filter { !$0.isBuiltIn },
                defaultStyleID: defaultStyleID,
                applicationStyleIDs: applicationStyleIDs,
                applicationModes: applicationModes,
                applicationProviderPreferences: applicationProviderPreferences,
                dismissedVocabularySuggestionIDs: dismissedVocabularySuggestionIDs.sorted()
            ))
            try data.write(to: fileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
            persistenceError = nil
        } catch {
            persistenceError = error.localizedDescription
        }
    }

    private func trimmed(_ value: String) -> String {
        value.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private func normalizedBundleIdentifier(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = trimmed(value)
        return normalized.isEmpty ? nil : normalized
    }

    private func normalizedOptionalText(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = trimmed(value)
        return normalized.isEmpty ? nil : normalized
    }

    private func scopedDictionary(for bundleIdentifier: String?) -> [DictionaryReplacement] {
        guard let bundleIdentifier else {
            return dictionary.filter { $0.bundleIdentifier == nil }
        }
        let scoped = dictionary.filter { $0.bundleIdentifier == bundleIdentifier }
        let scopedKeys = Set(scoped.map { normalizedPhrase($0.spoken) })
        return dictionary.filter {
            $0.bundleIdentifier == nil && !scopedKeys.contains(normalizedPhrase($0.spoken))
        } + scoped
    }

    private func scopedSnippets(for bundleIdentifier: String?) -> [Snippet] {
        guard let bundleIdentifier else {
            return snippets.filter { $0.bundleIdentifier == nil }
        }
        let scoped = snippets.filter { $0.bundleIdentifier == bundleIdentifier }
        let scopedKeys = Set(scoped.map { normalizedPhrase($0.trigger) })
        let scopedTypedTriggers = Set(scoped.compactMap(\.typedTrigger))
        return snippets.filter {
            $0.bundleIdentifier == nil
                && !scopedKeys.contains(normalizedPhrase($0.trigger))
                && ($0.typedTrigger.map { !scopedTypedTriggers.contains($0) } ?? true)
        } + scoped
    }

    private func normalizedPhrase(_ value: String) -> String {
        value.folding(
            options: [.caseInsensitive, .diacriticInsensitive],
            locale: .current
        )
    }

    private func discardMissingStyleSelections() {
        let availableIDs = Set(styles.map(\.id))
        if let defaultStyleID, !availableIDs.contains(defaultStyleID) {
            self.defaultStyleID = nil
        }
        applicationStyleIDs = applicationStyleIDs.filter {
            availableIDs.contains($0.value)
        }
    }

    private static func defaultFileURL() -> URL {
        let appSupport = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first!
        return appSupport
            .appendingPathComponent("VoiceScribe", isDirectory: true)
            .appendingPathComponent("personalization.json")
    }
}
