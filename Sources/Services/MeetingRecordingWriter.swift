import Foundation

protocol MeetingRecordingWriting: AnyObject {
    func start(id: UUID) throws -> String
    func append(_ data: Data) throws
    func finish() throws
    func discard()
}

enum MeetingRecordingWriterError: Error, LocalizedError {
    case notStarted

    var errorDescription: String? {
        "The meeting recording has not started."
    }
}

final class MeetingWAVRecordingWriter: MeetingRecordingWriting {
    private let directoryURL: URL
    private let lock = NSLock()
    private var fileHandle: FileHandle?
    private var fileURL: URL?
    private var audioBytes: UInt32 = 0

    init(directoryURL: URL? = nil) {
        if let directoryURL {
            self.directoryURL = directoryURL
        } else {
            let applicationSupport = FileManager.default.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first!
            self.directoryURL = applicationSupport
                .appendingPathComponent("VoiceScribe", isDirectory: true)
                .appendingPathComponent("Meetings", isDirectory: true)
                .appendingPathComponent("recordings", isDirectory: true)
        }
    }

    func start(id: UUID) throws -> String {
        try lock.withLock {
            try closeActiveFile()
            try FileManager.default.createDirectory(
                at: directoryURL,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: directoryURL.path
            )
            let filename = "\(id.uuidString).wav"
            let url = directoryURL.appendingPathComponent(filename)
            try wavHeader(audioBytes: 0).write(to: url, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: url.path
            )
            let handle = try FileHandle(forWritingTo: url)
            try handle.seekToEnd()
            fileHandle = handle
            fileURL = url
            audioBytes = 0
            return filename
        }
    }

    func append(_ data: Data) throws {
        guard !data.isEmpty else { return }
        try lock.withLock {
            guard let fileHandle else {
                throw MeetingRecordingWriterError.notStarted
            }
            try fileHandle.write(contentsOf: data)
            audioBytes = UInt32(
                min(UInt64(UInt32.max - 44), UInt64(audioBytes) + UInt64(data.count))
            )
        }
    }

    func finish() throws {
        try lock.withLock {
            guard let fileHandle, let fileURL else {
                throw MeetingRecordingWriterError.notStarted
            }
            try fileHandle.synchronize()
            try fileHandle.close()
            self.fileHandle = nil
            let headerHandle = try FileHandle(forWritingTo: fileURL)
            try headerHandle.seek(toOffset: 0)
            try headerHandle.write(contentsOf: wavHeader(audioBytes: audioBytes))
            try headerHandle.close()
            audioBytes = 0
        }
    }

    func discard() {
        lock.withLock {
            try? fileHandle?.close()
            fileHandle = nil
            if let fileURL {
                try? FileManager.default.removeItem(at: fileURL)
            }
            fileURL = nil
            audioBytes = 0
        }
    }

    private func closeActiveFile() throws {
        if let fileHandle {
            try fileHandle.close()
            self.fileHandle = nil
            if let fileURL {
                try? FileManager.default.removeItem(at: fileURL)
            }
        }
        fileURL = nil
        audioBytes = 0
    }

    private func wavHeader(audioBytes: UInt32) -> Data {
        var header = Data("RIFF".utf8)
        header.append(littleEndian: 36 &+ audioBytes)
        header.append(Data("WAVEfmt ".utf8))
        header.append(littleEndian: UInt32(16))
        header.append(littleEndian: UInt16(1))
        header.append(littleEndian: UInt16(1))
        header.append(littleEndian: UInt32(16_000))
        header.append(littleEndian: UInt32(32_000))
        header.append(littleEndian: UInt16(2))
        header.append(littleEndian: UInt16(16))
        header.append(Data("data".utf8))
        header.append(littleEndian: audioBytes)
        return header
    }
}

private extension Data {
    mutating func append<T: FixedWidthInteger>(littleEndian value: T) {
        var value = value.littleEndian
        Swift.withUnsafeBytes(of: &value) { append(contentsOf: $0) }
    }
}
