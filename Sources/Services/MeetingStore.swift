import Foundation

final class MeetingStore: ObservableObject {
    @Published private(set) var meetings: [MeetingRecord] = []
    @Published private(set) var persistenceError: String?

    private let fileURL: URL
    private let recordingsDirectoryURL: URL
    private var loadFailed = false

    init(
        fileURL: URL? = nil,
        recordingsDirectoryURL: URL? = nil
    ) {
        if let fileURL {
            self.fileURL = fileURL
        } else {
            let applicationSupport = FileManager.default.urls(
                for: .applicationSupportDirectory,
                in: .userDomainMask
            ).first!
            self.fileURL = applicationSupport
                .appendingPathComponent("VoiceScribe", isDirectory: true)
                .appendingPathComponent("Meetings", isDirectory: true)
                .appendingPathComponent("meetings.json")
        }
        self.recordingsDirectoryURL = recordingsDirectoryURL
            ?? self.fileURL.deletingLastPathComponent()
                .appendingPathComponent("recordings", isDirectory: true)
        load()
    }

    func save(_ meeting: MeetingRecord) {
        guard !meeting.transcript.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
            || meeting.recordingFilename != nil
        else {
            return
        }
        if let index = meetings.firstIndex(where: { $0.id == meeting.id }) {
            meetings[index] = meeting
        } else {
            meetings.insert(meeting, at: 0)
        }
        removeOverflowMeetings()
        persist()
    }

    func delete(at offsets: IndexSet) {
        for index in offsets {
            deleteRecording(filename: meetings[index].recordingFilename)
        }
        meetings.remove(atOffsets: offsets)
        persist()
    }

    func clear() {
        for filename in meetings.compactMap(\.recordingFilename) {
            deleteRecording(filename: filename)
        }
        meetings.removeAll()
        loadFailed = false
        persist()
    }

    func export(
        meeting: MeetingRecord,
        format: TranscriptionExportFormat
    ) throws -> Data {
        switch format {
        case .json:
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            return try encoder.encode(meeting)

        case .markdown:
            let title = meeting.title ?? "Mluva meeting"
            let decisions = markdownList(meeting.insights.decisions)
            let actionItems = markdownList(meeting.insights.actionItems)
            let speakers = meeting.speakers.map {
                "- [\(timestamp($0.startedAt))] \($0.speaker): \($0.text)"
            }.joined(separator: "\n")
            let markdown = """
            # \(title)

            - Captured: \(ISO8601DateFormatter().string(from: meeting.timestamp))
            - Provider: \(meeting.provider.displayName)
            - Language: \(meeting.language)
            - Audio: \(meeting.audioSources.map(\.rawValue).joined(separator: ", "))

            ## Summary

            \(meeting.insights.summary)

            ## Decisions

            \(decisions)

            ## Action items

            \(actionItems)

            ## Speakers

            \(speakers.isEmpty ? "Speaker labels unavailable." : speakers)

            ## Transcript

            \(meeting.transcript)
            """
            return Data(markdown.utf8)
        }
    }

    func recordingURL(for meeting: MeetingRecord) -> URL? {
        guard let filename = meeting.recordingFilename,
              !filename.isEmpty,
              URL(fileURLWithPath: filename).lastPathComponent == filename
        else {
            return nil
        }
        let url = recordingsDirectoryURL.appendingPathComponent(filename)
        return FileManager.default.fileExists(atPath: url.path) ? url : nil
    }

    private func load() {
        guard FileManager.default.fileExists(atPath: fileURL.path) else { return }
        let decoder = JSONDecoder()
        decoder.dateDecodingStrategy = .iso8601
        do {
            meetings = try decoder.decode(
                [MeetingRecord].self,
                from: Data(contentsOf: fileURL)
            )
            persistenceError = nil
            loadFailed = false
        } catch {
            persistenceError = error.localizedDescription
            loadFailed = true
        }
    }

    private func persist() {
        guard !loadFailed else { return }
        do {
            let directory = fileURL.deletingLastPathComponent()
            try FileManager.default.createDirectory(
                at: directory,
                withIntermediateDirectories: true,
                attributes: [.posixPermissions: 0o700]
            )
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o700],
                ofItemAtPath: directory.path
            )
            let encoder = JSONEncoder()
            encoder.dateEncodingStrategy = .iso8601
            encoder.outputFormatting = [.prettyPrinted, .sortedKeys]
            try encoder.encode(meetings).write(to: fileURL, options: .atomic)
            try FileManager.default.setAttributes(
                [.posixPermissions: 0o600],
                ofItemAtPath: fileURL.path
            )
            persistenceError = nil
        } catch {
            persistenceError = error.localizedDescription
        }
    }

    private func removeOverflowMeetings() {
        guard meetings.count > 200 else { return }
        let overflow = meetings.dropFirst(200)
        for filename in overflow.compactMap(\.recordingFilename) {
            deleteRecording(filename: filename)
        }
        meetings = Array(meetings.prefix(200))
    }

    private func deleteRecording(filename: String?) {
        guard let filename,
              !filename.isEmpty,
              URL(fileURLWithPath: filename).lastPathComponent == filename
        else {
            return
        }
        try? FileManager.default.removeItem(
            at: recordingsDirectoryURL.appendingPathComponent(filename)
        )
    }

    private func markdownList(_ values: [String]) -> String {
        values.isEmpty ? "None recorded." : values.map { "- \($0)" }.joined(separator: "\n")
    }

    private func timestamp(_ seconds: TimeInterval) -> String {
        let totalSeconds = max(0, Int(seconds.rounded()))
        return String(format: "%d:%02d", totalSeconds / 60, totalSeconds % 60)
    }
}
