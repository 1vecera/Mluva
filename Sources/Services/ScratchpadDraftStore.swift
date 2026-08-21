import Foundation

final class ScratchpadDraftStore: ObservableObject {
    static let shared = ScratchpadDraftStore()

    @Published private(set) var draft: ScratchpadDraft?
    @Published private(set) var persistenceError: String?

    private let fileURL: URL
    private let fileManager: FileManager

    init(fileURL: URL? = nil, fileManager: FileManager = .default) {
        self.fileManager = fileManager
        if let fileURL {
            self.fileURL = fileURL
        } else {
            let appSupport = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first!
            let directory = appSupport.appendingPathComponent(
                "VoiceScribe",
                isDirectory: true
            )
            self.fileURL = directory.appendingPathComponent("scratchpad-draft.json")
        }
        load()
    }

    func save(_ draft: ScratchpadDraft, persist: Bool) {
        self.draft = draft
        guard persist else {
            removePersistedDraft()
            return
        }

        do {
            try fileManager.createDirectory(
                at: fileURL.deletingLastPathComponent(),
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            let data = try encoder.encode(draft)
            try data.write(to: fileURL, options: .atomic)
            try fileManager.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
            persistenceError = nil
        } catch {
            persistenceError = error.localizedDescription
        }
    }

    func clear() {
        draft = nil
        removePersistedDraft()
        persistenceError = nil
    }

    private func load() {
        guard let data = try? Data(contentsOf: fileURL) else { return }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        do {
            draft = try decoder.decode(ScratchpadDraft.self, from: data)
            persistenceError = nil
        } catch {
            persistenceError = error.localizedDescription
        }
    }

    private func removePersistedDraft() {
        guard fileManager.fileExists(atPath: fileURL.path) else { return }
        try? fileManager.removeItem(at: fileURL)
    }
}
