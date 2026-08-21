import Foundation

enum TranscriptionExportFormat {
    case markdown
    case json

    var fileExtension: String {
        switch self {
        case .markdown: "md"
        case .json: "json"
        }
    }
}

final class TranscriptionStore: ObservableObject {
    @Published var entries: [TranscriptionEntry] = []
    @Published private(set) var persistenceError: String?

    private let fileURL: URL
    private let audioRetentionStore: AudioRetentionStore
    private let now: () -> Date
    private let hardensContainingDirectory: Bool
    private var retentionDays: Int
    private var loadFailed = false

    init(
        fileURL: URL? = nil,
        audioRetentionStore: AudioRetentionStore = .shared,
        retentionDays: Int? = nil,
        now: @escaping () -> Date = Date.init
    ) {
        self.audioRetentionStore = audioRetentionStore
        self.retentionDays = max(
            0,
            retentionDays ?? AppSettings.shared.historyRetentionDays
        )
        self.now = now
        if let url = fileURL {
            self.fileURL = url
            hardensContainingDirectory = false
        } else {
            let appSupport = FileManager.default.urls(
                for: .applicationSupportDirectory, in: .userDomainMask
            ).first!
            let dir = appSupport.appendingPathComponent("VoiceScribe", isDirectory: true)
            self.fileURL = dir.appendingPathComponent("transcriptions.json")
            hardensContainingDirectory = true
        }
        load()
        if pruneExpiredEntries() {
            persist()
        }
    }

    func save(text: String, duration: TimeInterval = 0) {
        save(entry: TranscriptionEntry(text: text, duration: duration))
    }

    func save(entry: TranscriptionEntry) {
        let rawText = entry.rawText.trimmingCharacters(in: .whitespacesAndNewlines)
        let deliveredText = entry.deliveredText.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !rawText.isEmpty
            || !deliveredText.isEmpty
            || entry.retainedAudioFilename != nil
        else {
            return
        }
        entries.insert(entry, at: 0)

        _ = pruneExpiredEntries()
        removeOverflowEntries()

        persist()
    }

    func update(entry: TranscriptionEntry) {
        guard let index = entries.firstIndex(where: { $0.id == entry.id }) else {
            save(entry: entry)
            return
        }
        entries[index] = entry
        _ = pruneExpiredEntries()
        persist()
    }

    func delete(at offsets: IndexSet) {
        for index in offsets {
            if let filename = entries[index].retainedAudioFilename {
                try? audioRetentionStore.delete(filename: filename)
            }
        }
        entries.remove(atOffsets: offsets)
        persist()
    }

    func clear() {
        for filename in entries.compactMap(\.retainedAudioFilename) {
            try? audioRetentionStore.delete(filename: filename)
        }
        entries.removeAll()
        loadFailed = false
        persist()
    }

    func updateRetention(days: Int) {
        retentionDays = max(0, days)
        if pruneExpiredEntries() {
            persist()
        }
    }

    func export(
        entry: TranscriptionEntry,
        format: TranscriptionExportFormat
    ) throws -> Data {
        switch format {
        case .json:
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            return try encoder.encode(entry)

        case .markdown:
            let title = entry.title ?? "Mluva transcript"
            let provider = switch entry.provider {
            case .automatic: "Automatic"
            case .apple: "Apple Speech"
            case .googleCloud: "Google Cloud"
            }
            let target = [entry.targetApplicationName, entry.targetBundleIdentifier]
                .compactMap { $0 }
                .joined(separator: " · ")
            let context = entry.contextSources
                .map(\.displayName)
                .joined(separator: ", ")
            let markdown = """
            # \(title)

            - Captured: \(ISO8601DateFormatter().string(from: entry.timestamp))
            - Provider: \(provider)
            - Language: \(entry.language)
            - Target: \(target.isEmpty ? "Unknown" : target)
            - Delivery: \(entry.deliveryOutcome.rawValue)
            - On-device context: \(context.isEmpty ? "None" : context)

            ## Delivered text

            \(entry.deliveredText)

            ## Raw transcript

            \(entry.rawText)
            """
            return Data(markdown.utf8)
        }
    }

    private func load() {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        do {
            let data = try Data(contentsOf: fileURL)
            entries = try decoder.decode([TranscriptionEntry].self, from: data)
            loadFailed = false
            persistenceError = nil
        } catch {
            loadFailed = true
            persistenceError = error.localizedDescription
        }
    }

    private func persist() {
        guard !loadFailed else { return }
        do {
            let directory = fileURL.deletingLastPathComponent()
            let directoryExisted = FileManager.default.fileExists(atPath: directory.path)
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            if hardensContainingDirectory || !directoryExisted {
                try FileManager.default.setAttributes(
                    [.posixPermissions: 0o700],
                    ofItemAtPath: directory.path
                )
            }
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = .prettyPrinted
            let data = try encoder.encode(entries)
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

    private func pruneExpiredEntries() -> Bool {
        guard retentionDays > 0 else { return false }
        let cutoff = now().addingTimeInterval(
            -TimeInterval(retentionDays) * 24 * 60 * 60
        )
        let expired = entries.filter { $0.timestamp < cutoff }
        guard !expired.isEmpty else { return false }
        deleteRetainedAudio(for: expired)
        entries.removeAll { $0.timestamp < cutoff }
        return true
    }

    private func removeOverflowEntries() {
        guard entries.count > 500 else { return }
        let overflow = Array(entries.dropFirst(500))
        deleteRetainedAudio(for: overflow)
        entries = Array(entries.prefix(500))
    }

    private func deleteRetainedAudio(for entries: [TranscriptionEntry]) {
        for filename in entries.compactMap(\.retainedAudioFilename) {
            try? audioRetentionStore.delete(filename: filename)
        }
    }
}
