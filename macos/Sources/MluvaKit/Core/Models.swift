import Foundation

// Mirrors of the shared core's serialized types. Field names follow the Rust serde names, so
// files written on macOS and Linux stay interchangeable.

/// `config.json`. Every field round-trips, including those only the Linux app uses.
public struct AppConfig: Codable, Equatable, Sendable {
    public var language_code: String
    public var transcription_model: String
    public var transcription_provider: String
    public var transcription_base_url: String
    public var transcription_api_key_env: String
    public var transcription_remote_model: String?
    public var local_model: String
    public var local_device: String
    public var transcription_chunk_seconds: Int
    public var widget_position: String
    public var widget_lines: Int
    public var widget_opacity: Int
    public var history_sidebar_visible: Bool
    public var welcome_completed: Bool
    public var time_format: String
    public var microphone_target: String?
    public var system_audio_target: String?
    public var global_recording_key: String
    public var spoken_commands_enabled: Bool
    public var audio_retention_policy: String
    public var incognito_mode: Bool
    public var history_retention_days: Int
}

public struct HistoryEntry: Codable, Equatable, Sendable, Identifiable, Hashable {
    public var identifier: String
    public var created_at: String
    public var raw_text: String
    public var delivered_text: String
    public var mode: String
    public var language_code: String
    public var transcription_id: String?
    public var delivery_outcome: String
    public var title: String?
    public var application_identifier: String?
    public var recognition_route: String?
    public var recognition_fallback_reason: String?
    public var recognition_ms: Int?
    public var delivery_ms: Int?

    public var id: String { identifier }

    public var createdAt: Date? {
        (try? Date.ISO8601FormatStyle(includingFractionalSeconds: true).parse(created_at))
            ?? (try? Date.ISO8601FormatStyle().parse(created_at))
    }

    /// The saved title, else the first words of what was said.
    public var displayTitle: String {
        if let title, !title.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty { return title }
        let words = delivered_text.split(whereSeparator: \.isWhitespace).prefix(8).joined(separator: " ")
        return words.isEmpty ? "Untitled dictation" : words
    }
}

public struct HistoryInput: Codable, Sendable {
    public var raw_text: String
    public var delivered_text: String
    public var mode: String = "dictation"
    public var language_code: String
    public var transcription_id: String?
    public var delivery_outcome: String
    public var audio_retention_policy: String?
    public var application_identifier: String?
    public var recognition_route: String?
    public var recognition_fallback_reason: String?
    public var recognition_ms: Int?
    public var enhancement_ms: Int?
    public var delivery_ms: Int?

    public init(
        raw: String, delivered: String, language: String, outcome: String,
        transcriptionID: String? = nil, application: String? = nil, route: String? = nil,
        fallbackReason: String? = nil, recognitionMs: Int? = nil, deliveryMs: Int? = nil
    ) {
        raw_text = raw
        delivered_text = delivered
        language_code = language
        delivery_outcome = outcome
        transcription_id = transcriptionID
        application_identifier = application
        recognition_route = route
        recognition_fallback_reason = fallbackReason
        recognition_ms = recognitionMs
        delivery_ms = deliveryMs
    }
}

public struct TimedWord: Codable, Equatable, Sendable {
    public var text: String
    public var start_seconds: Double
    public var end_seconds: Double
}

public struct TranscriptionResult: Codable, Equatable, Sendable {
    public var text: String
    public var language_code: String
    public var transcription_id: String?
    public var audio_duration_seconds: Double?
    public var words: [TimedWord]?
}

public enum AgentStatus: String, Codable, Sendable {
    case idle, working, blocked, done, unknown

    public init(from decoder: any Decoder) throws {
        let raw = try decoder.singleValueContainer().decode(String.self)
        self = AgentStatus(rawValue: raw) ?? .unknown
    }
}

/// A running agent session as the shared target model saves it: Herdr's pane flat, as
/// choices saved before targets existed were written, and the agent's own session and process.
/// What a transport needs and never saves, such as a Claude Code inbox, stays in the core.
public struct AgentTarget: Codable, Equatable, Sendable, Hashable {
    public var socket: String
    public var server_epoch: String
    public var pane_id: String
    public var `guard`: String
    public var name: String
    public var title: String
    /// `claude`, `claude_desktop`, `codex`, `codex_desktop` or `unknown`.
    public var kind: String
    public var project: String
    public var status: AgentStatus
    public var session_id: String
    public var pid: UInt32
    /// The process's start time as the kernel counts it, when known.
    public var started: String?
    /// An application surface reached by link, such as `claude_new_chat`.
    public var app: String?

    public init(
        socket: String = "", server_epoch: String = "", pane_id: String = "", guard: String = "",
        name: String, title: String = "", kind: String, project: String = "", status: AgentStatus,
        session_id: String = "", pid: UInt32 = 0, started: String? = nil, app: String? = nil
    ) {
        self.socket = socket
        self.server_epoch = server_epoch
        self.pane_id = pane_id
        self.guard = `guard`
        self.name = name
        self.title = title
        self.kind = kind
        self.project = project
        self.status = status
        self.session_id = session_id
        self.pid = pid
        self.started = started
        self.app = app
    }

    private var inHerdr: Bool { !socket.isEmpty || !pane_id.isEmpty }

    private func samePane(_ other: AgentTarget) -> Bool {
        socket == other.socket && server_epoch == other.server_epoch && pane_id == other.pane_id
    }

    /// The core's `same_instance`: an app surface is that surface; otherwise the product, its
    /// own session ID and process, and Herdr's guard or pane only when neither has a session.
    public func sameInstance(as other: AgentTarget) -> Bool {
        if app != nil || other.app != nil { return app == other.app && kind == other.kind }
        switch (session_id.isEmpty, other.session_id.isEmpty) {
        case (false, false):
            let (mine, theirs) = (started ?? "", other.started ?? "")
            return kind == other.kind && session_id == other.session_id && pid == other.pid
                && (mine.isEmpty || theirs.isEmpty || mine == theirs)
        case (let noSession, let otherNoSession):
            let (guarded, otherGuarded) = (inHerdr && !`guard`.isEmpty, other.inHerdr && !other.guard.isEmpty)
            if guarded && otherGuarded { return samePane(other) && `guard` == other.guard }
            if !guarded && !otherGuarded && noSession && otherNoSession {
                return inHerdr && other.inHerdr && samePane(other)
            }
            return false
        }
    }
}

/// One discovered target with the core's own judgements about it.
public struct AgentSession: Codable, Equatable, Sendable, Identifiable {
    public var target: AgentTarget
    public var display_name: String
    /// The product, as people say it: `Claude Code`, `Codex CLI`, `Claude Desktop`.
    public var kind: String
    /// Where it runs, in a few words: `Herdr w1:p2`, `terminal`, `app`.
    public var place: String
    /// How far a dictation reaches it: `send`, `queue`, `prefill` or `draft_only`.
    public var delivery: String
    public var verb: String
    /// What the route does with a message when it is not a plain send.
    public var delivery_hint: String?
    public var draft_only: Bool
    public var draft_hint: String
    public var can_receive: Bool
    public var spoken_status: String

    public init(
        target: AgentTarget, display_name: String, kind: String = "Claude Code", place: String = "terminal",
        delivery: String, verb: String, delivery_hint: String? = nil, draft_only: Bool, draft_hint: String,
        can_receive: Bool, spoken_status: String
    ) {
        self.target = target
        self.display_name = display_name
        self.kind = kind
        self.place = place
        self.delivery = delivery
        self.verb = verb
        self.delivery_hint = delivery_hint
        self.draft_only = draft_only
        self.draft_hint = draft_hint
        self.can_receive = can_receive
        self.spoken_status = spoken_status
    }

    public var id: String {
        "\(target.socket)|\(target.pane_id)|\(target.session_id)|\(target.pid)|\(target.app ?? "")"
    }

    /// The project folder name, for telling sessions apart.
    public var project: String {
        target.project.isEmpty ? "" : URL(fileURLWithPath: target.project).lastPathComponent
    }
}

public struct Discovery: Codable, Equatable, Sendable {
    public var sessions: [AgentSession]
    public var herdr: Bool
    public var guarded: Bool
    /// The panel's line for what discovery found.
    public var summary: String
    public var errors: [String]
}

/// `agent-voice.json`.
public struct VoiceSettings: Codable, Equatable, Sendable {
    public var enabled: Bool
    public var socket: String?
    public var selected: AgentTarget?
    public var hold_key: String
    public var draft_key: String
    public var spoken_updates: Bool
    public var voice_id: String
}

public struct DictionaryReplacement: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var spoken: String
    public var written: String
    public var bundleIdentifier: String?
    public var caseBehavior: String
}

public struct Snippet: Codable, Equatable, Sendable, Identifiable {
    public var id: String
    public var trigger: String
    public var expansion: String
    public var typedTrigger: String?
    public var bundleIdentifier: String?
}

public struct PersonalizationState: Codable, Equatable, Sendable {
    public var dictionary: [DictionaryReplacement]
    public var snippets: [Snippet]

    public init(dictionary: [DictionaryReplacement] = [], snippets: [Snippet] = []) {
        self.dictionary = dictionary
        self.snippets = snippets
    }
}

public struct VocabularySuggestion: Codable, Equatable, Sendable, Identifiable {
    public var identifier: String
    public var spoken: String
    public var written: String
    public var occurrences: Int
    public var id: String { identifier }
}

/// The agent route's current attempt, as the core reports it.
public struct RouteState: Codable, Equatable, Sendable {
    /// `recording`, `finalizing`, `grace`, `submitting`, `delivered`, `held`, `unconfirmed`
    /// or `draft`.
    public var phase: String
    public var id: String
    public var reason: String?
    public var deadline_ms: UInt64?
    public var target: AgentTarget
    public var display_name: String
    /// The level the target is reached at: `send`, `queue` or `prefill`.
    public var delivery: String
    /// What the route does with a message when it is not a plain send.
    public var delivery_hint: String?
    /// Where to look after an uncertain delivery.
    public var place_to_check: String
    /// A queued message the agent has since taken into a turn.
    public var taken: Bool
    public var text: String
    public var history_id: String?

    public init(
        phase: String, id: String, reason: String? = nil, deadline_ms: UInt64? = nil, target: AgentTarget,
        display_name: String, delivery: String, delivery_hint: String? = nil, place_to_check: String = "the session",
        taken: Bool = false, text: String, history_id: String? = nil
    ) {
        self.phase = phase
        self.id = id
        self.reason = reason
        self.deadline_ms = deadline_ms
        self.target = target
        self.display_name = display_name
        self.delivery = delivery
        self.delivery_hint = delivery_hint
        self.place_to_check = place_to_check
        self.taken = taken
        self.text = text
        self.history_id = history_id
    }
}
