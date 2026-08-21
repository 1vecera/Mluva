import Foundation

enum AudioRetentionPolicy: String, Codable, CaseIterable, Sendable {
    case never
    case failures
    case always

    func shouldRetain(deliverySucceeded: Bool) -> Bool {
        switch self {
        case .never:
            return false
        case .failures:
            return !deliverySucceeded
        case .always:
            return true
        }
    }
}

enum AudioRetentionStoreError: Error {
    case invalidFilename
}

final class AudioRetentionStore {
    static let shared = AudioRetentionStore()

    private let directoryURL: URL
    private let fileManager: FileManager

    init(directoryURL: URL? = nil, fileManager: FileManager = .default) {
        self.fileManager = fileManager
        if let directoryURL {
            self.directoryURL = directoryURL
        } else {
            let appSupport = fileManager.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first!
            self.directoryURL = appSupport
                .appendingPathComponent("VoiceScribe", isDirectory: true)
                .appendingPathComponent("RetainedAudio", isDirectory: true)
        }
    }

    @discardableResult
    func save(_ audio: Data, for entryID: UUID) throws -> String {
        try fileManager.createDirectory(
            at: directoryURL,
            withIntermediateDirectories: true,
            attributes: [.posixPermissions: 0o700]
        )
        let filename = "\(entryID.uuidString).pcm"
        try audio.write(to: directoryURL.appendingPathComponent(filename), options: .atomic)
        return filename
    }

    func load(filename: String) throws -> Data {
        try Data(contentsOf: validatedURL(filename: filename))
    }

    func exists(filename: String) -> Bool {
        guard let url = try? validatedURL(filename: filename) else { return false }
        return fileManager.fileExists(atPath: url.path)
    }

    func delete(filename: String) throws {
        let url = try validatedURL(filename: filename)
        guard fileManager.fileExists(atPath: url.path) else { return }
        try fileManager.removeItem(at: url)
    }

    private func validatedURL(filename: String) throws -> URL {
        guard filename == URL(fileURLWithPath: filename).lastPathComponent,
              filename.hasSuffix(".pcm")
        else {
            throw AudioRetentionStoreError.invalidFilename
        }
        return directoryURL.appendingPathComponent(filename)
    }
}

final class AudioCaptureBuffer: @unchecked Sendable {
    private let lock = NSLock()
    private var audio = Data()

    func reset() {
        lock.withLock { audio = Data() }
    }

    func append(_ data: Data) {
        lock.withLock { audio.append(data) }
    }

    func snapshot() -> Data {
        lock.withLock { audio }
    }
}
