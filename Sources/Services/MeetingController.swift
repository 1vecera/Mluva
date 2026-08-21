import Foundation

final class MeetingController: ObservableObject, @unchecked Sendable {
    @Published private(set) var state: RecordingState = .idle
    @Published private(set) var partialText = ""
    @Published private(set) var recordingStartedAt: Date?
    @Published private(set) var activeProviderKind: TranscriptionProviderKind?
    @Published private(set) var lastMeeting: MeetingRecord?
    @Published var error: String?

    private let settings: AppSettings
    private let providerFactory: any TranscriptionProviderBuilding
    private let audioCapture: any MeetingAudioCapturing
    private let meetingStore: MeetingStore
    private let recordingWriter: any MeetingRecordingWriting
    private let insightExtractor: MeetingInsightExtractor
    private let now: () -> Date
    private var provider: (any TranscriptionProvider)?
    private var meetingID: UUID?
    private var recordingFilename: String?
    private var sessionIsIncognito = false

    init(
        settings: AppSettings = .shared,
        providerFactory: any TranscriptionProviderBuilding = DefaultTranscriptionProviderFactory(),
        audioCapture: any MeetingAudioCapturing = MeetingAudioCaptureCoordinator(),
        meetingStore: MeetingStore,
        recordingWriter: any MeetingRecordingWriting = MeetingWAVRecordingWriter(),
        insightExtractor: MeetingInsightExtractor = MeetingInsightExtractor(),
        now: @escaping () -> Date = Date.init
    ) {
        self.settings = settings
        self.providerFactory = providerFactory
        self.audioCapture = audioCapture
        self.meetingStore = meetingStore
        self.recordingWriter = recordingWriter
        self.insightExtractor = insightExtractor
        self.now = now
    }

    func start() async throws {
        guard state == .idle else { return }
        state = .starting
        error = nil
        partialText = ""
        lastMeeting = nil
        sessionIsIncognito = settings.incognitoMode
        let provider: any TranscriptionProvider
        do {
            provider = try providerFactory.makeProvider(settings: settings)
        } catch {
            state = .idle
            self.error = error.localizedDescription
            throw error
        }
        self.provider = provider
        activeProviderKind = provider.kind
        provider.onEvent = { [weak self] event in
            DispatchQueue.main.async {
                self?.partialText = event.text
            }
        }
        audioCapture.onAudioChunk = { [weak self, weak provider] data in
            provider?.appendAudio(data)
            do {
                try self?.recordingWriter.append(data)
            } catch {
                DispatchQueue.main.async { self?.error = error.localizedDescription }
            }
        }
        audioCapture.onError = { [weak self] error in
            DispatchQueue.main.async { self?.error = error.localizedDescription }
        }

        do {
            let id = UUID()
            meetingID = id
            recordingFilename = try recordingWriter.start(id: id)
            try await provider.start()
            try await audioCapture.start()
            recordingStartedAt = now()
            state = .recording
        } catch {
            provider.cancel()
            recordingWriter.discard()
            self.provider = nil
            activeProviderKind = nil
            recordingFilename = nil
            state = .idle
            self.error = error.localizedDescription
            throw error
        }
    }

    func stop() async throws -> MeetingRecord? {
        guard state == .recording,
              let provider,
              let meetingID,
              let startedAt = recordingStartedAt
        else {
            return nil
        }
        state = .stopping
        await audioCapture.stop()
        do {
            let transcript = try await provider.finish()
                .trimmingCharacters(in: .whitespacesAndNewlines)
            try recordingWriter.finish()
            let language = TranscriptionLanguage(identifier: settings.language)
            let meeting = MeetingRecord(
                id: meetingID,
                transcript: transcript,
                insights: insightExtractor.extract(from: transcript),
                timestamp: startedAt,
                duration: now().timeIntervalSince(startedAt),
                provider: provider.kind,
                language: provider.kind == .apple
                    ? language.appleLocaleIdentifier
                    : language.googleLanguageCode,
                audioSources: [.microphone, .system],
                recordingFilename: sessionIsIncognito ? nil : recordingFilename
            )
            if sessionIsIncognito {
                recordingWriter.discard()
            } else {
                meetingStore.save(meeting)
            }
            lastMeeting = meeting
            reset()
            return meeting
        } catch {
            recordingWriter.discard()
            self.error = error.localizedDescription
            reset()
            throw error
        }
    }

    func cancel() async {
        guard state != .idle else { return }
        await audioCapture.stop()
        provider?.cancel()
        recordingWriter.discard()
        reset()
    }

    private func reset() {
        provider = nil
        meetingID = nil
        recordingFilename = nil
        sessionIsIncognito = false
        recordingStartedAt = nil
        activeProviderKind = nil
        state = .idle
    }
}
