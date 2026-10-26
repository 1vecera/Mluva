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

/// A running agent session. `guard` is Herdr's send transport; empty means draft only.
public struct Recipient: Codable, Equatable, Sendable, Hashable {
    public var socket: String
    public var server_epoch: String
    public var pane_id: String
    public var `guard`: String
    public var name: String
    public var title: String
    public var kind: String
    public var project: String
    public var status: AgentStatus
    public var session_id: String
    public var pid: UInt32

    public init(
        socket: String, server_epoch: String, pane_id: String, guard: String, name: String, title: String,
        kind: String, project: String, status: AgentStatus, session_id: String, pid: UInt32
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
    }

    /// The core's `same_instance`: guarded identity, else native session and process.
    public func sameInstance(as other: Recipient) -> Bool {
        if `guard`.isEmpty != other.guard.isEmpty { return false }
        let herdr = socket == other.socket && server_epoch == other.server_epoch && pane_id == other.pane_id
        if !`guard`.isEmpty { return herdr && `guard` == other.guard }
        if !session_id.isEmpty || !other.session_id.isEmpty {
            return session_id == other.session_id && pid == other.pid
        }
        return herdr
    }
}

/// One discovered session with the core's own judgements about it.
public struct AgentSession: Codable, Equatable, Sendable, Identifiable {
    public var recipient: Recipient
    public var display_name: String
    public var draft_only: Bool
    public var draft_hint: String
    public var can_receive: Bool
    public var spoken_status: String

    public init(
        recipient: Recipient, display_name: String, draft_only: Bool, draft_hint: String,
        can_receive: Bool, spoken_status: String
    ) {
        self.recipient = recipient
        self.display_name = display_name
        self.draft_only = draft_only
        self.draft_hint = draft_hint
        self.can_receive = can_receive
        self.spoken_status = spoken_status
    }

    public var id: String { "\(recipient.socket)|\(recipient.pane_id)|\(recipient.session_id)|\(recipient.pid)" }

    /// The project folder name, for telling sessions apart.
    public var project: String {
        URL(fileURLWithPath: recipient.project).lastPathComponent
    }
}

public struct Discovery: Codable, Equatable, Sendable {
    public var sessions: [AgentSession]
    public var herdr: Bool
    public var guarded: Bool
    public var errors: [String]
}

/// `agent-voice.json`.
public struct VoiceSettings: Codable, Equatable, Sendable {
    public var enabled: Bool
    public var socket: String?
    public var selected: Recipient?
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
    public var phase: String
    public var id: String
    public var reason: String?
    public var deadline_ms: UInt64?
    public var recipient: Recipient
    public var text: String
    public var history_id: String?

    public init(
        phase: String, id: String, reason: String?, deadline_ms: UInt64?, recipient: Recipient,
        text: String, history_id: String?
    ) {
        self.phase = phase
        self.id = id
        self.reason = reason
        self.deadline_ms = deadline_ms
        self.recipient = recipient
        self.text = text
        self.history_id = history_id
    }
}
