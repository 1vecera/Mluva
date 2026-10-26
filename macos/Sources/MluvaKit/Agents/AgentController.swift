import Foundation
import Observation

/// Agent sessions and the guarded send route. Discovery reads Claude Code's own session
/// registry and, when present, Herdr; only Herdr's guarded prompt can send.
@MainActor
@Observable
public final class AgentController {
    public private(set) var sessions: [AgentSession] = []
    public private(set) var herdrReachable = false
    public private(set) var herdrGuarded = false
    public private(set) var settings: VoiceSettings?
    public private(set) var settingsError: String?
    public private(set) var route: RouteState?
    /// The last send outcome the core reported: `sent`, `rejected` or `unconfirmed`.
    public private(set) var lastOutcome: String?

    private let core: CoreBridge
    private var polling: Task<Void, Never>?
    private var ticking: Task<Void, Never>?
    private var events: ListenerToken?
    private var refreshing = false
    private let recordingKey: () -> String

    public init(core: CoreBridge = .shared, recordingKey: @escaping () -> String) {
        self.core = core
        self.recordingKey = recordingKey
        events = core.listen { [weak self] event in
            guard case .route(_, let outcome) = event else { return }
            Task { @MainActor in
                self?.lastOutcome = outcome
                self?.updateRoute()
            }
        }
    }

    public func loadSettings() {
        do {
            settings = try core.call("voice.load", as: VoiceSettings.self)
            settingsError = nil
        } catch {
            settingsError = error.localizedDescription
        }
    }

    public func save(_ next: VoiceSettings) throws {
        try core.call("voice.save", ["settings": try JSONValue(encoding: next), "recording_key": .string(recordingKey())])
        settings = next
    }

    /// The remembered session if it is still present; a replacement never takes its place.
    public var selected: AgentSession? {
        guard let remembered = settings?.selected else { return nil }
        return sessions.first { $0.recipient.sameInstance(as: remembered) }
    }

    public func select(_ session: AgentSession) {
        guard var next = settings else { return }
        next.selected = session.recipient
        if session.recipient.guard.isEmpty == false { next.enabled = true }
        try? save(next)
    }

    public func refresh() async {
        guard !refreshing else { return }
        refreshing = true
        defer { refreshing = false }
        var params: JSONValue = [:]
        if let socket = settings?.socket { params = ["socket": .string(socket)] }
        guard let found = try? await core.task("agents.discover", params, as: Discovery.self) else { return }
        if found.sessions != sessions { sessions = found.sessions }
        herdrReachable = found.herdr
        herdrGuarded = found.guarded
        let recipients = try? JSONValue(encoding: found.sessions.map(\.recipient))
        _ = try? core.call("route.observe", ["recipients": recipients ?? []])
        updateRoute()
    }

    /// Poll every two seconds while something shows sessions.
    public func startPolling() {
        guard polling == nil else { return }
        polling = Task { [weak self] in
            while !Task.isCancelled {
                await self?.refresh()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }

    public func stopPolling() {
        polling?.cancel()
        polling = nil
    }

    // MARK: Guarded route

    public static func now() -> UInt64 { UInt64(Date().timeIntervalSince1970 * 1_000) }

    /// Begin an agent capture for a session that can receive. Draft-only sessions throw.
    public func begin(_ session: AgentSession) throws -> String {
        let id = try core.call("route.begin", ["recipient": try JSONValue(encoding: session.recipient), "now_ms": .init(Self.now())])
        updateRoute()
        startTicking()
        return id.string ?? ""
    }

    public func release() {
        _ = try? core.call("route.release", ["now_ms": .init(Self.now())])
        updateRoute()
    }

    public func recognized(id: String, text: String, historyID: String?) {
        _ = try? core.call("route.recognized", [
            "id": .string(id), "text": .string(text), "history_id": .init(historyID), "now_ms": .init(Self.now()),
        ])
        updateRoute()
    }

    /// Keep the text as a draft: Cancel, Keep draft, editing, or a changed recipient.
    public func disarm(_ reason: String) {
        _ = try? core.call("route.disarm", ["reason": .string(reason)])
        updateRoute()
    }

    public func abandon(_ reason: String) {
        _ = try? core.call("route.abandon", ["reason": .string(reason)])
        updateRoute()
    }

    private func startTicking() {
        ticking?.cancel()
        lastOutcome = nil
        ticking = Task { [weak self] in
            while !Task.isCancelled {
                guard let self else { return }
                let state = try? self.core.call("route.tick", ["now_ms": .init(Self.now())])
                self.route = try? state?.decode(RouteState.self)
                if let phase = self.route?.phase, ["draft", "sent", "unconfirmed"].contains(phase) { return }
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }

    private func updateRoute() {
        route = try? core.call("route.state").decode(RouteState.self)
    }
}
