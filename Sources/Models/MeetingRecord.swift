import Foundation

enum MeetingAudioSource: String, Codable, CaseIterable, Sendable {
    case microphone
    case system
}

struct MeetingSpeakerSegment: Codable, Equatable, Sendable {
    let speaker: String
    let text: String
    let startedAt: TimeInterval
    let endedAt: TimeInterval

    init(
        speaker: String,
        text: String,
        startedAt: TimeInterval,
        endedAt: TimeInterval
    ) {
        self.speaker = speaker
        self.text = text
        self.startedAt = max(0, startedAt)
        self.endedAt = max(self.startedAt, endedAt)
    }
}

struct MeetingInsights: Codable, Equatable, Sendable {
    static let empty = MeetingInsights(summary: "", decisions: [], actionItems: [])

    let summary: String
    let decisions: [String]
    let actionItems: [String]
}

struct MeetingRecord: Codable, Equatable, Identifiable, Sendable {
    let id: UUID
    let title: String?
    let transcript: String
    let speakers: [MeetingSpeakerSegment]
    let insights: MeetingInsights
    let timestamp: Date
    let duration: TimeInterval
    let provider: TranscriptionProviderKind
    let language: String
    let audioSources: [MeetingAudioSource]
    let recordingFilename: String?

    init(
        id: UUID = UUID(),
        title: String? = nil,
        transcript: String,
        speakers: [MeetingSpeakerSegment] = [],
        insights: MeetingInsights = .empty,
        timestamp: Date = Date(),
        duration: TimeInterval,
        provider: TranscriptionProviderKind,
        language: String,
        audioSources: [MeetingAudioSource],
        recordingFilename: String? = nil
    ) {
        self.id = id
        self.title = title
        self.transcript = transcript
        self.speakers = speakers
        self.insights = insights
        self.timestamp = timestamp
        self.duration = max(0, duration)
        self.provider = provider
        self.language = language
        self.audioSources = MeetingAudioSource.allCases.filter(audioSources.contains)
        self.recordingFilename = recordingFilename
    }

    func renamed(_ title: String?) -> MeetingRecord {
        let normalizedTitle = title?.trimmingCharacters(in: .whitespacesAndNewlines)
        return MeetingRecord(
            id: id,
            title: normalizedTitle?.isEmpty == true ? nil : normalizedTitle,
            transcript: transcript,
            speakers: speakers,
            insights: insights,
            timestamp: timestamp,
            duration: duration,
            provider: provider,
            language: language,
            audioSources: audioSources,
            recordingFilename: recordingFilename
        )
    }
}
