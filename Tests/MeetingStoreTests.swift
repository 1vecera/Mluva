import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Meeting archive")
struct MeetingStoreTests {
    @Test("Meetings persist separately with transcript and insights")
    func persistsMeetingRecord() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-meeting-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let fileURL = directory.appendingPathComponent("meetings.json")
        let record = MeetingRecord(
            transcript: "We decided to ship Friday.",
            speakers: [MeetingSpeakerSegment(
                speaker: "Speaker 1",
                text: "We decided to ship Friday.",
                startedAt: 0,
                endedAt: 2.4
            )],
            insights: MeetingInsights(
                summary: "The team set the launch date.",
                decisions: ["Ship Friday."],
                actionItems: []
            ),
            timestamp: Date(timeIntervalSince1970: 1_800_000_000),
            duration: 2.4,
            provider: .googleCloud,
            language: "en-US",
            audioSources: [.microphone, .system]
        )

        MeetingStore(fileURL: fileURL).save(record)
        let reloaded = MeetingStore(fileURL: fileURL)

        #expect(reloaded.meetings == [record])
        #expect((try FileManager.default.attributesOfItem(atPath: fileURL.path)[.posixPermissions] as? NSNumber)?.intValue == 0o600)
        #expect((try FileManager.default.attributesOfItem(atPath: directory.path)[.posixPermissions] as? NSNumber)?.intValue == 0o700)
    }

    @Test("Deleting a meeting also deletes only its retained recording")
    func deleteRemovesMeetingRecording() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-meeting-delete-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let fileURL = directory.appendingPathComponent("meetings.json")
        let recordings = directory.appendingPathComponent("recordings")
        try FileManager.default.createDirectory(
            at: recordings,
            withIntermediateDirectories: true
        )
        let recordingURL = recordings.appendingPathComponent("meeting.pcm")
        try Data([0x01, 0x02]).write(to: recordingURL)
        let store = MeetingStore(
            fileURL: fileURL,
            recordingsDirectoryURL: recordings
        )
        store.save(MeetingRecord(
            transcript: "Meeting",
            duration: 1,
            provider: .apple,
            language: "en-US",
            audioSources: [.microphone],
            recordingFilename: "meeting.pcm"
        ))

        store.delete(at: IndexSet(integer: 0))

        #expect(store.meetings.isEmpty)
        #expect(!FileManager.default.fileExists(atPath: recordingURL.path))
    }

    @Test("A retained meeting recording resolves only inside its private archive")
    func resolvesRetainedRecording() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-meeting-recording-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let recordings = directory.appendingPathComponent("recordings")
        try FileManager.default.createDirectory(
            at: recordings,
            withIntermediateDirectories: true
        )
        let recordingURL = recordings.appendingPathComponent("meeting.wav")
        try Data([0x01, 0x02]).write(to: recordingURL)
        let store = MeetingStore(
            fileURL: directory.appendingPathComponent("meetings.json"),
            recordingsDirectoryURL: recordings
        )
        let retained = MeetingRecord(
            transcript: "Meeting",
            duration: 1,
            provider: .apple,
            language: "en-US",
            audioSources: [.microphone, .system],
            recordingFilename: "meeting.wav"
        )
        let escaped = MeetingRecord(
            transcript: "Unsafe",
            duration: 1,
            provider: .apple,
            language: "en-US",
            audioSources: [.microphone],
            recordingFilename: "../meeting.wav"
        )

        #expect(store.recordingURL(for: retained) == recordingURL)
        #expect(store.recordingURL(for: escaped) == nil)
    }

    @Test("Corrupt meeting archive remains untouched")
    func preservesCorruptArchive() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-meeting-corrupt-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        try FileManager.default.createDirectory(
            at: directory,
            withIntermediateDirectories: true
        )
        let fileURL = directory.appendingPathComponent("meetings.json")
        let corruptData = Data("not a meeting archive".utf8)
        try corruptData.write(to: fileURL)
        let store = MeetingStore(fileURL: fileURL)

        #expect(store.persistenceError != nil)
        store.save(MeetingRecord(
            transcript: "New meeting",
            duration: 1,
            provider: .apple,
            language: "en-US",
            audioSources: [.microphone]
        ))

        #expect(try Data(contentsOf: fileURL) == corruptData)
    }
}
