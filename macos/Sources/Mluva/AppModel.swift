import AppKit
import MluvaKit
import Observation
import SwiftUI

/// Owns the app's controllers, shortcuts and floating panels.
@MainActor
@Observable
final class AppModel {
    let paths: AppPaths
    let permissions = PermissionCenter()
    let agents: AgentController
    let dictation: DictationController
    let annotation: AnnotationController
    private let hotkeys = HotkeyCenter()

    private(set) var config: AppConfig?
    private(set) var configError: String?
    private(set) var hasKey = false
    private(set) var recent: [HistoryEntry] = []
    private(set) var refusedKeys: [HotkeyAction] = []
    private(set) var startupError: String?
    /// The wheel is open and showing this page.
    private(set) var wheelOpen = false
    var wheelPage = 0

    @ObservationIgnored private var pill: FloatingPanel?
    @ObservationIgnored private var wheel: FloatingPanel?
    @ObservationIgnored private var hidePill: Task<Void, Never>?
    @ObservationIgnored var openWindow: ((String, String?) -> Void)?
    @ObservationIgnored var openSettingsAction: (() -> Void)?
    /// Evidence runs show the ready state without a key or microphone grant.
    var evidenceReady = false
    let isEvidence = Evidence.directory != nil

    init(paths: AppPaths = AppModel.defaultPaths()) {
        self.paths = paths
        let agents = AgentController()
        self.agents = agents
        dictation = DictationController(paths: paths, agents: agents)
        annotation = AnnotationController(paths: paths)
        do {
            try CoreBridge.shared.call("open", [
                "config_dir": .string(paths.config.path), "data_dir": .string(paths.data.path),
                "herdr_config_home": .string(paths.herdrConfigHome.path),
            ])
        } catch {
            startupError = error.localizedDescription
        }
        reloadConfig()
        agents.loadSettings()
        adoptMacAgentKey()
        dictation.onRecordingChange = { [weak self] recording in
            self?.annotation.recordingChanged(recording, incognito: self?.dictation.incognito ?? false)
        }
        dictation.appendix = { [weak self] result in
            await self?.annotation.finish(transcript: result)
        }
        agents.recordingKey = { [weak self] in self?.config?.global_recording_key ?? "F9" }
        hotkeys.onPress = { [weak self] action in self?.handle(action) }
        bindKeys()
        refreshKey()
        refreshHistory()
    }

    /// Evidence runs never touch real settings or History.
    static func defaultPaths() -> AppPaths {
        var environment = ProcessInfo.processInfo.environment
        if environment["MLUVA_EVIDENCE"] != nil, environment["MLUVA_HOME"] == nil {
            environment["MLUVA_HOME"] = FileManager.default.temporaryDirectory
                .appending(path: "mluva-evidence-\(UUID().uuidString)").path
        }
        return .standard(environment: environment)
    }

    // MARK: Settings

    func reloadConfig() {
        do {
            let loaded = try CoreBridge.shared.call("config.load")
            config = try loaded["config"]?.decode(AppConfig.self)
            configError = loaded["error"]?.string
        } catch {
            configError = error.localizedDescription
        }
    }

    func save(_ next: AppConfig) throws {
        try CoreBridge.shared.call("config.save", ["config": try JSONValue(encoding: next)])
        config = next
        bindKeys()
    }

    func saveVoice(_ next: VoiceSettings) throws {
        try agents.save(next)
        bindKeys()
    }

    /// The Mac's agent wheel defaults to F12, as on Daniel's Linux setup; F8 is a media key.
    private func adoptMacAgentKey() {
        guard var settings = agents.settings,
              !FileManager.default.fileExists(atPath: paths.config.appending(path: "agent-voice.json").path),
              settings.hold_key == "F8" else { return }
        settings.hold_key = "F12"
        try? agents.save(settings)
    }

    func bindKeys() {
        guard !isEvidence else { return }
        var keys: [HotkeyAction: String] = [.dictate: config?.global_recording_key ?? "F9"]
        if let voice = agents.settings {
            keys[.agent] = voice.hold_key
            keys[.keepDraft] = voice.draft_key
        }
        refusedKeys = hotkeys.bind(keys)
    }

    func refreshKey() {
        hasKey = speechKey() != nil
    }

    func refreshHistory() {
        recent = (try? CoreBridge.shared.call("history.recent", ["limit": 200], as: [HistoryEntry].self)) ?? []
    }

    var needsSetup: Bool {
        if evidenceReady { return false }
        return !hasKey || permissions.state(.microphone) != .granted
    }

    // MARK: Shortcuts

    func handle(_ action: HotkeyAction) {
        switch action {
        case .dictate:
            if wheelOpen {
                let session = agents.selected
                closeWheel()
                Task { await dictate(.document(session)) }
            } else if dictation.phase == .recording || dictation.phase == .preparing {
                Task { await dictation.toggle(.manual) }
            } else {
                let target = TextDelivery.captureTarget()
                Task { await dictate(.target(target)) }
            }
        case .agent:
            if dictation.phase == .recording || dictation.phase == .preparing {
                Task { await dictation.toggle(.manual) }
            } else if wheelOpen {
                recordForSelection()
            } else {
                openWheel()
            }
        case .keepDraft:
            if agents.route != nil { agents.disarm("kept") }
            if dictation.phase == .recording { Task { await dictation.stop() } }
        }
    }

    func dictate(_ destination: Destination) async {
        showPill()
        await dictation.start(destination)
        if case .failed = dictation.phase { schedulePillHide(after: 6) }
    }

    func stopOrStart(manual: Bool) {
        Task {
            if dictation.phase == .recording { await dictation.stop(); finishPill() } else { await dictate(.manual) }
        }
    }

    // MARK: Wheel

    func openWheel() {
        guard !wheelOpen else { return }
        wheelOpen = true
        wheelPage = 0
        agents.startPolling()
        let panel = wheel ?? FloatingPanel(keyable: true) { [weak self] in
            self.map { SessionWheelView(model: $0) }
        }
        wheel = panel
        panel.onEscape = { [weak self] in self?.closeWheel() }
        panel.fillScreen(NSScreen.withMouse)
        panel.present()
    }

    func closeWheel() {
        guard wheelOpen else { return }
        wheelOpen = false
        agents.stopPolling()
        wheel?.dismiss()
    }

    /// Second agent key press: record for the highlighted session.
    func recordForSelection() {
        guard let session = agents.selected else { return }
        closeWheel()
        Task { await dictate(session.can_receive ? .agent(session) : .document(session)) }
    }

    func choose(_ session: AgentSession) {
        agents.select(session)
    }

    // MARK: Pill

    func showPill() {
        hidePill?.cancel()
        let panel = pill ?? FloatingPanel(keyable: false) { [weak self] in
            self.map { RecordingPillView(model: $0) }
        }
        pill = panel
        panel.placeBottomCenter(of: NSScreen.withMouse, size: CGSize(width: 620, height: 260))
        panel.present()
        watchPill()
    }

    private func watchPill() {
        withObservationTracking {
            _ = dictation.phase
            _ = agents.route?.phase
        } onChange: { [weak self] in
            Task { @MainActor in
                self?.finishPill()
                self?.watchPill()
            }
        }
    }

    func finishPill() {
        let route = agents.route?.phase
        if route == "grace" || route == "submitting" || route == "recording" || route == "finalizing" { return }
        switch dictation.phase {
        case .finished, .noSpeech: schedulePillHide(after: route == "draft" ? 5 : 2.4)
        case .failed: schedulePillHide(after: 7)
        case .idle: schedulePillHide(after: 0.2)
        default: break
        }
    }

    private func schedulePillHide(after seconds: Double) {
        hidePill?.cancel()
        hidePill = Task { [weak self] in
            try? await Task.sleep(for: .seconds(seconds))
            guard !Task.isCancelled, let self, !self.dictation.isBusy else { return }
            self.pill?.dismiss()
            self.refreshHistory()
        }
    }
}

extension NSScreen {
    /// The screen under the pointer, where the person is looking.
    static var withMouse: NSScreen? {
        let point = NSEvent.mouseLocation
        return screens.first { NSMouseInRect(point, $0.frame, false) } ?? main
    }
}
