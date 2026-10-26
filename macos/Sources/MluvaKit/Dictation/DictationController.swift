import Foundation
import Observation
import os

/// Where a recording's text goes when it is final.
public enum Destination: Sendable {
    /// A global shortcut: paste into the captured field when it is unchanged, else copy.
    case target(CapturedTarget?)
    /// The menu or pill button: copy only, because clicking Mluva moved focus.
    case manual
    /// An agent session, through its own transport after the two-second cancel window. Never
    /// copied or pasted.
    case agent(AgentSession)
    /// An editable Mluva transcript only: no copy, paste or send (draft-only sessions).
    case document(AgentSession?)

    public var label: String {
        switch self {
        case .target(let target): target.map { "Paste into \($0.applicationName)" } ?? "Copy to clipboard"
        case .manual: "Copy to clipboard"
        case .agent(let session): "\(session.verb) to \(session.display_name)"
        case .document(let session): session.map { "Draft for \($0.display_name)" } ?? "Mluva draft"
        }
    }

    var isAgent: Bool { if case .agent = self { true } else { false } }
}

public enum DictationPhase: Equatable, Sendable {
    case idle
    case preparing
    case recording
    case finalizing
    case finished(String)
    case noSpeech
    case failed(String)

    public var isActive: Bool { self == .preparing || self == .recording || self == .finalizing }
}

/// How a recording ended, for what it owns besides its text.
public enum CaptureEnd: Equatable, Sendable {
    /// Its text reached History under this conversation, or could not be saved (nil).
    case saved(String?)
    /// Cancelled, failed or silent: nothing it made is kept.
    case discarded
}

/// One recording at a time, from the shortcut to History and delivery.
@MainActor
@Observable
public final class DictationController {
    public private(set) var phase: DictationPhase = .idle
    /// Committed recognition followed by the volatile tail; volatile text is never delivered.
    public private(set) var committed = ""
    public private(set) var volatile = ""
    public private(set) var level: Float = 0
    public private(set) var startedAt: Date?
    public private(set) var destination: Destination = .manual
    public private(set) var lastEntry: HistoryEntry?
    public private(set) var lastText = ""
    /// A fallback or delivery explanation worth showing.
    public private(set) var notice: String?
    public private(set) var incognito = false
    /// The current recording's identifier, owning what it makes until History does.
    public private(set) var captureID: String?

    /// Text appended to the delivered message, such as a screen annotation summary.
    public var appendix: (@MainActor (TranscriptionResult) async -> String?)?
    /// Called when a recording starts or ends, for overlays such as screen annotation.
    public var onRecordingChange: (@MainActor (Bool) -> Void)?
    /// Called once per recording that started, when its outcome is known.
    public var onCaptureEnd: (@MainActor (String, CaptureEnd) -> Void)?

    private let core: CoreBridge
    private let paths: AppPaths
    private let agents: AgentController
    private var microphone: MicrophoneCapture?
    private var session: Session?
    private var generation = 0
    private var events: ListenerToken?

    /// State shared with the audio thread.
    private final class Session: @unchecked Sendable {
        let lock = OSAllocatedUnfairLock()
        let wav: WAVWriter?
        var memory = Data()
        var realtime: String?
        var realtimeFailed: String?
        /// PCM that arrived before the realtime session was ready.
        var pending = Data()
        var level: Float = 0
        var bytes = 0
        let snapshot: JSONValue
        let config: AppConfig
        let key: String
        var routeID: String?

        init(wav: WAVWriter?, snapshot: JSONValue, config: AppConfig, key: String) {
            self.wav = wav
            self.snapshot = snapshot
            self.config = config
            self.key = key
        }
    }

    public init(core: CoreBridge = .shared, paths: AppPaths, agents: AgentController) {
        self.core = core
        self.paths = paths
        self.agents = agents
        events = core.listen { [weak self] event in
            guard case .realtimePreview(let id, let committed, let volatile) = event else { return }
            Task { @MainActor in
                guard let self, self.session?.realtime == id, self.phase.isActive else { return }
                self.committed = committed
                self.volatile = volatile
            }
        }
    }

    public var previewText: String {
        [committed, volatile].filter { !$0.isEmpty }.joined(separator: " ")
    }

    public var isBusy: Bool { phase.isActive }

    /// Start, or stop when already recording. A second press while preparing cancels.
    public func toggle(_ destination: @autoclosure () -> Destination) async {
        switch phase {
        case .recording: await stop()
        case .preparing: cancel()
        case .finalizing: break
        default: await start(destination())
        }
    }

    public func start(_ destination: Destination) async {
        guard !phase.isActive else { return }
        generation += 1
        let generation = generation
        self.destination = destination
        committed = ""
        volatile = ""
        notice = nil
        level = 0
        phase = .preparing
        captureID = UUID().uuidString.lowercased()
        do {
            let config = try core.call("config.load")["config"]?.decode(AppConfig.self)
            guard let config else { throw BridgeError("Mluva's settings could not be read.") }
            guard let key = speechKey() else {
                throw BridgeError("Add your ElevenLabs API key in Settings → Speech to start dictating.")
            }
            incognito = config.incognito_mode
            var application: String?
            if case .target(let target) = destination { application = target?.bundleIdentifier }
            let snapshot = try core.call("dictation.freeze", ["application": .init(application)])
            let wav = config.incognito_mode ? nil : try WAVWriter(
                url: paths.recordings.appending(path: "\(UUID().uuidString.lowercased()).wav"))
            let session = Session(wav: wav, snapshot: snapshot, config: config, key: key)
            if case .agent(let agent) = destination {
                session.routeID = try agents.begin(agent)
            }
            self.session = session
            startRealtime(session, generation: generation)
            let microphone = MicrophoneCapture()
            self.microphone = microphone
            try await microphone.start { [weak self] pcm in
                self?.capture(pcm, into: session)
            }
            guard generation == self.generation, phase == .preparing else {
                microphone.stop()
                return
            }
            startedAt = Date()
            phase = .recording
            onRecordingChange?(true)
            pollLevel(generation)
        } catch {
            fail(error.localizedDescription, generation: generation)
        }
    }

    /// Stop recording and finish: recognition, preparation, History, then delivery.
    public func stop() async {
        guard phase == .recording, let session else { return }
        let generation = generation
        phase = .finalizing
        microphone?.stop()
        microphone = nil
        onRecordingChange?(false)
        if destination.isAgent { agents.release() }
        try? session.wav?.close()
        let started = Date()
        let (realtime, failure) = session.lock.withLock { (session.realtime, session.realtimeFailed) }
        var result: TranscriptionResult?
        var fallback: String? = failure
        if let realtime {
            do {
                let finished = try await core.task("realtime.finish", ["session": .string(realtime)])
                result = try finished["transcription"]?.decode(TranscriptionResult.self)
            } catch {
                fallback = error.localizedDescription
            }
        }
        if result == nil {
            guard let wav = session.wav else {
                return fail("Realtime recognition failed (\(fallback ?? "unavailable")). Incognito keeps audio off disk, so there is no batch fallback.", generation: generation)
            }
            notice = "Realtime recognition was unavailable, so the whole recording was sent as one upload."
            do {
                result = try await core.task("scribe.transcribe", [
                    "key": .string(session.key), "path": .string(wav.url.path),
                    "language": .string(session.config.language_code),
                    "model": .string(session.config.transcription_model),
                ], as: TranscriptionResult.self)
            } catch {
                return fail(error.localizedDescription, generation: generation, keepAudio: true)
            }
        }
        guard generation == self.generation, let result else { return }
        let recognitionMs = Int(Date().timeIntervalSince(started) * 1_000)
        await finish(result, session: session, recognitionMs: recognitionMs, fallback: fallback)
    }

    /// Cancel before delivery. Audio already sent to the provider cannot be recalled.
    public func cancel() {
        guard phase.isActive || agents.route?.phase == "grace" else { return }
        generation += 1
        microphone?.stop()
        microphone = nil
        if let session {
            let realtime = session.lock.withLock { session.realtime }
            if let realtime { _ = try? core.call("realtime.cancel", ["session": .string(realtime)]) }
            try? session.wav?.close()
            if let url = session.wav?.url { try? FileManager.default.removeItem(at: url) }
        }
        if destination.isAgent { agents.abandon("cancelled") }
        session = nil
        if phase.isActive { onRecordingChange?(false) }
        end(.discarded)
        phase = .idle
        committed = ""
        volatile = ""
    }

    /// Dismiss a finished state back to idle.
    public func reset() {
        guard !phase.isActive else { return }
        phase = .idle
    }

    // MARK: Internals

    nonisolated private func capture(_ pcm: Data, into session: Session) {
        session.lock.withLock {
            try? session.wav?.append(pcm)
            if session.wav == nil { session.memory.append(pcm) }
            session.bytes += pcm.count
            session.level = pcmLevel(pcm)
        }
        let realtime: String? = session.lock.withLock {
            if session.realtime == nil, session.realtimeFailed == nil {
                session.pending.append(pcm)
                return nil
            }
            return session.realtime
        }
        if let realtime, !core.realtimeAudio(session: realtime, pcm: pcm) {
            session.lock.withLock { session.realtimeFailed = session.realtimeFailed ?? "the realtime connection closed" }
        }
    }

    private func startRealtime(_ session: Session, generation: Int) {
        Task { [core] in
            do {
                let started = try await core.task("realtime.start", [
                    "key": .string(session.key), "language": .string(session.config.language_code),
                ])
                guard let id = started["session"]?.string else { throw BridgeError("No realtime session.") }
                let pending = session.lock.withLock { () -> Data in
                    session.realtime = id
                    defer { session.pending = Data() }
                    return session.pending
                }
                if !pending.isEmpty, !core.realtimeAudio(session: id, pcm: pending) {
                    session.lock.withLock { session.realtimeFailed = "the realtime connection closed" }
                }
            } catch {
                session.lock.withLock {
                    session.realtimeFailed = error.localizedDescription
                    session.pending = Data()
                }
            }
        }
    }

    private func pollLevel(_ generation: Int) {
        Task { [weak self] in
            while let self, self.generation == generation, self.phase == .recording {
                self.level = self.session.map { session in session.lock.withLock { session.level } } ?? 0
                try? await Task.sleep(for: .milliseconds(50))
            }
        }
    }

    private func finish(_ result: TranscriptionResult, session: Session, recognitionMs: Int, fallback: String?) async {
        let raw = result.text
        let policy = session.config.audio_retention_policy
        let keepAudio = !session.config.incognito_mode && policy == "always"
        if raw.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty {
            if !keepAudio, let url = session.wav?.url { try? FileManager.default.removeItem(at: url) }
            if let routeID = session.routeID { agents.recognized(id: routeID, text: "", historyID: nil) }
            end(.discarded)
            phase = .noSpeech
            self.session = nil
            return
        }
        let processed = (try? core.call("dictation.process", ["snapshot": session.snapshot, "raw": .string(raw)]).string) ?? raw
        var delivered = processed
        if let appendix, let extra = await appendix(result) {
            delivered = (try? core.call("dictation.append", ["text": .string(processed), "appendix": .string(extra)]).string) ?? processed
        }
        let started = Date()
        var receipt = DeliveryReceipt(outcome: .ready, guidance: "")
        switch destination {
        case .target(let target):
            receipt = await TextDelivery.deliver(delivered, to: target, allowInsertion: true)
        case .manual:
            receipt = await TextDelivery.deliver(delivered, to: nil, allowInsertion: false)
        case .agent(let agent):
            receipt.guidance = "\(agent.verb) to \(agent.display_name) in two seconds."
        case .document:
            receipt.guidance = "Saved as an editable draft. Nothing was copied or sent."
        }
        let deliveryMs = Int(Date().timeIntervalSince(started) * 1_000)
        var entry: HistoryEntry?
        if !session.config.incognito_mode {
            var application: String?
            if case .target(let target) = destination { application = target?.bundleIdentifier }
            var input = HistoryInput(
                raw: raw, delivered: processed, language: result.language_code.isEmpty ? session.config.language_code : result.language_code,
                outcome: receipt.outcome.rawValue, transcriptionID: result.transcription_id, application: application,
                route: fallback == nil ? "elevenlabs-realtime" : "elevenlabs-batch-fallback",
                fallbackReason: fallback.map { _ in "realtime-unavailable" },
                recognitionMs: recognitionMs, deliveryMs: deliveryMs)
            input.audio_retention_policy = policy
            entry = try? core.call("history.add", ["input": try JSONValue(encoding: input)], as: HistoryEntry.self)
        }
        if !keepAudio, let url = session.wav?.url { try? FileManager.default.removeItem(at: url) }
        if let routeID = session.routeID {
            agents.recognized(id: routeID, text: delivered, historyID: entry?.identifier)
        }
        end(.saved(entry?.identifier))
        lastEntry = entry
        lastText = processed
        committed = processed
        volatile = ""
        if notice == nil, !receipt.guidance.isEmpty { notice = receipt.guidance }
        phase = .finished(receipt.guidance)
        self.session = nil
    }

    private func fail(_ message: String, generation: Int, keepAudio: Bool = false) {
        guard generation == self.generation else { return }
        microphone?.stop()
        microphone = nil
        if let session {
            try? session.wav?.close()
            let policy = session.config.audio_retention_policy
            if let url = session.wav?.url, !(keepAudio && policy != "never") {
                try? FileManager.default.removeItem(at: url)
            }
            if session.routeID != nil { agents.abandon("interrupted") }
        }
        if phase == .recording { onRecordingChange?(false) }
        end(.discarded)
        session = nil
        phase = .failed(message)
    }

    private func end(_ outcome: CaptureEnd) {
        guard let captureID else { return }
        self.captureID = nil
        onCaptureEnd?(captureID, outcome)
    }
}

// MARK: Evidence

extension DictationController {
    /// Show a synthetic state for screenshots and recordings; no audio or provider is used.
    @_spi(Evidence)
    public func present(
        _ phase: DictationPhase, committed: String = "", volatile: String = "", level: Float = 0,
        destination: Destination = .manual, startedAt: Date? = nil, lastText: String = "", notice: String? = nil
    ) {
        self.destination = destination
        self.committed = committed
        self.volatile = volatile
        self.level = level
        self.startedAt = startedAt
        self.lastText = lastText
        self.notice = notice
        let wasRecording = self.phase == .recording
        self.phase = phase
        if wasRecording != (phase == .recording) { onRecordingChange?(phase == .recording) }
    }
}
