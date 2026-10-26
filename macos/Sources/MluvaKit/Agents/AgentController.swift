import Foundation
import Observation

/// Agent sessions and the route a dictation takes to one. The core's shared target model
/// discovers them (Herdr when it runs, Claude Code's own registry, live Codex threads and a new
/// Claude chat) and delivers through each target's own transport: Herdr's guarded prompt,
/// Claude Code's inbox queue, Codex's send-if-idle queue or a prefilled Claude chat.
@MainActor
@Observable
public final class AgentController {
    public private(set) var sessions: [AgentSession] = []
    public private(set) var herdrReachable = false
    public private(set) var herdrGuarded = false
    /// The core's line for what discovery found.
    public private(set) var summary = ""
    public private(set) var settings: VoiceSettings?
    public private(set) var settingsError: String?
    public private(set) var route: RouteState?
    /// The last delivery outcome the core reported: `accepted`, `taken`, `held`, `busy`,
    /// `rejected` or `unconfirmed`.
    public private(set) var lastOutcome: String?

    private let core: CoreBridge
    private var polling: Task<Void, Never>?
    private var ticking: Task<Void, Never>?
    private var events: ListenerToken?
    private var refreshing = false
    /// The dictation key, which agent keys must not reuse.
    public var recordingKey: () -> String

    public init(core: CoreBridge = .shared, recordingKey: @escaping () -> String = { "F9" }) {
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
        return sessions.first { $0.target.sameInstance(as: remembered) }
    }

    public func select(_ session: AgentSession) {
        guard var next = settings else { return }
        next.selected = session.target
        if !session.draft_only { next.enabled = true }
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
        summary = found.summary
        // The core moved a pending attempt to what reaches its session now.
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

    // MARK: Route

    public static func now() -> UInt64 { UInt64(Date().timeIntervalSince1970 * 1_000) }

    /// Begin an agent capture for a session that can receive, as the latest discovery found it.
    /// Draft-only and vanished sessions throw.
    public func begin(_ session: AgentSession) throws -> String {
        let id = try core.call("route.begin", ["target": try JSONValue(encoding: session.target), "now_ms": .init(Self.now())])
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

    /// A second press during the cancel window: deliver now, to the same target.
    public func sendNow() {
        route = try? core.call("route.send_now", ["now_ms": .init(Self.now())]).decode(RouteState.self)
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
                if let phase = self.route?.phase, ["draft", "delivered", "held", "unconfirmed"].contains(phase) { return }
                try? await Task.sleep(for: .milliseconds(100))
            }
        }
    }

    private func updateRoute() {
        route = try? core.call("route.state").decode(RouteState.self)
    }
}

// MARK: Evidence

extension AgentController {
    /// Show synthetic sessions and route state for screenshots; discovery and Herdr are not used.
    @_spi(Evidence)
    public func present(sessions: [AgentSession], selected: AgentTarget?, route: RouteState?) {
        self.sessions = sessions
        if var settings { settings.selected = selected; self.settings = settings }
        self.route = route
    }
}
