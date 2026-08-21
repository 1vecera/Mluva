import Testing
import Foundation
@testable import VoiceScribeMac

@Suite("Transcription Store")
struct TranscriptionStoreTests {
    let tempDir: URL
    let fileURL: URL

    init() {
        tempDir = FileManager.default.temporaryDirectory
            .appendingPathComponent("voicescribe-test-\(UUID().uuidString)")
        try? FileManager.default.createDirectory(at: tempDir, withIntermediateDirectories: true)
        fileURL = tempDir.appendingPathComponent("test-transcriptions.json")
    }

    @Test("Save adds entry to store")
    func saveAddsEntry() {
        let store = TranscriptionStore(fileURL: fileURL)
        store.save(text: "Hello world")

        #expect(store.entries.count == 1)
        #expect(store.entries[0].text == "Hello world")
    }

    @Test("Save inserts newest at front")
    func saveInsertsAtFront() {
        let store = TranscriptionStore(fileURL: fileURL)
        store.save(text: "First")
        store.save(text: "Second")

        #expect(store.entries.count == 2)
        #expect(store.entries[0].text == "Second")
        #expect(store.entries[1].text == "First")
    }

    @Test("Empty/whitespace text is not saved")
    func emptyTextIgnored() {
        let store = TranscriptionStore(fileURL: fileURL)
        store.save(text: "")
        store.save(text: "   ")
        store.save(text: "\n")

        #expect(store.entries.isEmpty)
    }

    @Test("Clear removes all entries")
    func clearRemovesAll() {
        let store = TranscriptionStore(fileURL: fileURL)
        store.save(text: "one")
        store.save(text: "two")

        store.clear()

        #expect(store.entries.isEmpty)
    }

    @Test("Delete at offset removes correct entry")
    func deleteAtOffset() {
        let store = TranscriptionStore(fileURL: fileURL)
        store.save(text: "one")
        store.save(text: "two")
        store.save(text: "three")

        store.delete(at: IndexSet(integer: 1))

        #expect(store.entries.count == 2)
        #expect(store.entries[0].text == "three")
        #expect(store.entries[1].text == "one")
    }

    @Test("Entries persist across instances")
    func persistence() {
        let initialStore = TranscriptionStore(fileURL: fileURL)
        initialStore.save(text: "persisted")

        let reloadedStore = TranscriptionStore(fileURL: fileURL)
        #expect(reloadedStore.entries.count == 1)
        #expect(reloadedStore.entries[0].text == "persisted")
    }

    @Test("History is stored in an owner-only directory and file")
    func historyStorageIsPrivate() throws {
        let historyDirectory = tempDir.appendingPathComponent(
            "private-history",
            isDirectory: true
        )
        let historyURL = historyDirectory.appendingPathComponent("transcriptions.json")
        let store = TranscriptionStore(fileURL: historyURL)

        store.save(text: "private transcript")

        #expect(store.persistenceError == nil)
        let fileAttributes = try FileManager.default.attributesOfItem(atPath: historyURL.path)
        #expect((fileAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
        let directoryAttributes = try FileManager.default.attributesOfItem(
            atPath: historyDirectory.path
        )
        #expect((directoryAttributes[.posixPermissions] as? NSNumber)?.intValue == 0o700)
    }

    @Test("Expired history and its retained audio are permanently removed")
    func retentionPrunesExpiredHistory() throws {
        let now = Date(timeIntervalSince1970: 1_800_000_000)
        let audioDirectory = tempDir.appendingPathComponent("retention-audio")
        let audioStore = AudioRetentionStore(directoryURL: audioDirectory)
        let oldEntryID = UUID()
        let oldFilename = try audioStore.save(Data([0x01]), for: oldEntryID)
        let initialStore = TranscriptionStore(
            fileURL: fileURL,
            audioRetentionStore: audioStore,
            retentionDays: 0,
            now: { now }
        )
        initialStore.save(entry: TranscriptionEntry(
            id: oldEntryID,
            rawText: "expired",
            deliveredText: "Expired",
            timestamp: now.addingTimeInterval(-40 * 24 * 60 * 60),
            provider: .apple,
            language: "en",
            mode: .dictation,
            deliveryOutcome: .delivered,
            retainedAudioFilename: oldFilename
        ))
        initialStore.save(entry: TranscriptionEntry(
            rawText: "recent",
            deliveredText: "Recent",
            timestamp: now.addingTimeInterval(-2 * 24 * 60 * 60),
            provider: .apple,
            language: "en",
            mode: .dictation,
            deliveryOutcome: .delivered
        ))

        let retainedStore = TranscriptionStore(
            fileURL: fileURL,
            audioRetentionStore: audioStore,
            retentionDays: 30,
            now: { now }
        )

        #expect(retainedStore.entries.map(\.rawText) == ["recent"])
        #expect(!audioStore.exists(filename: oldFilename))
    }

    @Test("Unreadable history is reported and never overwritten implicitly")
    func corruptHistoryRemainsRecoverable() throws {
        let invalidHistory = Data("not valid history".utf8)
        try invalidHistory.write(to: fileURL, options: .atomic)
        let store = TranscriptionStore(fileURL: fileURL)

        #expect(store.persistenceError != nil)

        store.save(text: "new transcript")

        #expect(try Data(contentsOf: fileURL) == invalidHistory)
        #expect(store.entries.map(\.text) == ["new transcript"])
    }

    @Test("Rich recovery metadata persists")
    func richMetadataPersists() throws {
        let initialStore = TranscriptionStore(fileURL: fileURL)
        initialStore.save(entry: TranscriptionEntry(
            rawText: "use post grass",
            deliveredText: "Use Postgres",
            duration: 1.8,
            provider: .apple,
            language: "en-US",
            mode: .dictation,
            targetApplicationName: "Xcode",
            targetBundleIdentifier: "com.apple.dt.Xcode",
            deliveryOutcome: .delivered
        ))

        let reloadedStore = TranscriptionStore(fileURL: fileURL)
        let entry = try #require(reloadedStore.entries.first)
        #expect(entry.rawText == "use post grass")
        #expect(entry.deliveredText == "Use Postgres")
        #expect(entry.provider == .apple)
        #expect(entry.targetApplicationName == "Xcode")
    }

    @Test("Deleting history also permanently deletes retained audio")
    func deleteRemovesRetainedAudio() throws {
        let audioDirectory = tempDir.appendingPathComponent("retained-audio")
        let audioStore = AudioRetentionStore(directoryURL: audioDirectory)
        let entryID = UUID()
        let filename = try audioStore.save(Data([0x01, 0x02]), for: entryID)
        let store = TranscriptionStore(
            fileURL: fileURL,
            audioRetentionStore: audioStore
        )
        store.save(entry: TranscriptionEntry(
            id: entryID,
            rawText: "failed",
            deliveredText: "",
            provider: .googleCloud,
            language: "en-US",
            mode: .dictation,
            deliveryOutcome: .failed,
            retainedAudioFilename: filename
        ))

        store.delete(at: IndexSet(integer: store.entries.startIndex))

        #expect(store.entries.isEmpty)
        #expect(audioStore.exists(filename: filename) == false)
    }

    @Test("History exports include capture provenance and both transcript forms")
    func exportsHistoryEntry() throws {
        let store = TranscriptionStore(fileURL: fileURL)
        let entry = TranscriptionEntry(
            rawText: "raw post grass",
            deliveredText: "Raw Postgres",
            provider: .googleCloud,
            language: "en-US",
            mode: .dictation,
            targetApplicationName: "Code",
            targetBundleIdentifier: "com.microsoft.VSCode",
            deliveryOutcome: .delivered,
            contextSources: [.application, .windowTitle]
        ).renamed("Technical note")

        let data = try store.export(entry: entry, format: .markdown)
        let exported = try #require(String(data: data, encoding: .utf8))

        #expect(exported.contains("# Technical note"))
        #expect(exported.contains("Google Cloud"))
        #expect(exported.contains("raw post grass"))
        #expect(exported.contains("Raw Postgres"))
        #expect(exported.contains("com.microsoft.VSCode"))
        #expect(exported.contains("On-device context: application, window title"))
    }

    @Test("Max 500 entries enforced")
    func maxEntries() {
        let store = TranscriptionStore(fileURL: fileURL)
        for i in 0..<510 {
            store.save(text: "entry \(i)")
        }

        #expect(store.entries.count == 500)
        #expect(store.entries[0].text == "entry 509")
    }
}
