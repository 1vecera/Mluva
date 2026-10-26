import MluvaKit
import SwiftUI

/// Settings in standard grouped forms. Every change saves at once.
struct SettingsView: View {
    let model: AppModel

    var body: some View {
        TabView {
            Tab("General", systemImage: "gearshape") { GeneralSettings(model: model) }
            Tab("Speech", systemImage: "waveform") { SpeechSettings(model: model) }
            Tab("Shortcuts", systemImage: "keyboard") { ShortcutSettings(model: model) }
            Tab("Agents", systemImage: "terminal") { AgentSettings(model: model) }
            Tab("Vocabulary", systemImage: "character.book.closed") { VocabularySettings() }
            Tab("Privacy", systemImage: "hand.raised") { PermissionSettings(model: model) }
        }
        .frame(width: 560)
        .frame(minHeight: 460)
    }
}

/// Edits one field of the shared config and saves it immediately.
@MainActor
private func update(_ model: AppModel, _ change: (inout AppConfig) -> Void) {
    guard var config = model.config else { return }
    change(&config)
    try? model.save(config)
}

private struct GeneralSettings: View {
    let model: AppModel
    @State private var launchAtLogin = LaunchAtLogin.isEnabled
    @State private var loginError: String?

    var body: some View {
        Form {
            Section {
                Toggle("Open Mluva at login", isOn: $launchAtLogin)
                    .onChange(of: launchAtLogin) { _, enabled in
                        do {
                            try LaunchAtLogin.set(enabled)
                            loginError = nil
                        } catch {
                            loginError = error.localizedDescription
                            launchAtLogin = LaunchAtLogin.isEnabled
                        }
                    }
                if LaunchAtLogin.needsApproval {
                    Text("Approve Mluva in System Settings → General → Login Items.").font(.callout).foregroundStyle(.secondary)
                }
                if let loginError { Text(loginError).font(.callout).foregroundStyle(.orange) }
            }
            Section("Privacy") {
                Toggle(isOn: Binding(get: { model.config?.incognito_mode ?? false }, set: { value in update(model) { $0.incognito_mode = value } })) {
                    Text("Incognito")
                    Text("No History, no retained audio and no annotation. Recognition still uses ElevenLabs.")
                }
                Picker("Keep recordings", selection: Binding(get: { model.config?.audio_retention_policy ?? "failures" }, set: { value in update(model) { $0.audio_retention_policy = value } })) {
                    Text("Only when recognition fails").tag("failures")
                    Text("Always").tag("always")
                    Text("Never").tag("never")
                }
                Picker("Keep History", selection: Binding(get: { model.config?.history_retention_days ?? 0 }, set: { value in update(model) { $0.history_retention_days = value } })) {
                    Text("Forever").tag(0)
                    Text("30 days").tag(30)
                    Text("90 days").tag(90)
                    Text("1 year").tag(365)
                }
            }
            Section {
                LabeledContent("Data") {
                    Button("Show in Finder") { NSWorkspace.shared.activateFileViewerSelecting([model.paths.data]) }
                }
            } footer: {
                Text("Settings and History use the same files as Mluva on Linux.").font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
    }
}

private struct SpeechSettings: View {
    let model: AppModel
    @State private var key = ""
    @State private var message: String?
    @State private var stored = KeychainItem.elevenLabs.read() != nil

    private static let languages: [(String, String)] = [
        ("eng", "English"), ("ces", "Czech"), ("slk", "Slovak"), ("deu", "German"), ("fra", "French"),
        ("spa", "Spanish"), ("ita", "Italian"), ("pol", "Polish"), ("ukr", "Ukrainian"), ("auto", "Detect automatically"),
    ]

    var body: some View {
        Form {
            Section {
                LabeledContent("Provider", value: "ElevenLabs Scribe v2")
                SecureField("API key", text: $key, prompt: Text(stored ? "Saved in your Keychain" : "Paste your ElevenLabs key"))
                HStack {
                    Button("Save key") { saveKey() }.disabled(key.isEmpty)
                    if stored {
                        Button("Remove", role: .destructive) {
                            KeychainItem.elevenLabs.delete()
                            stored = false
                            model.refreshKey()
                            message = "Removed from the Keychain."
                        }
                    }
                    Spacer()
                    if let message { Text(message).font(.callout).foregroundStyle(.secondary) }
                }
            } header: {
                Text("Recognition")
            } footer: {
                Text("Audio goes to ElevenLabs while you dictate. The key stays in your login Keychain and never enters settings, History or logs.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section {
                Picker("Language", selection: Binding(get: { model.config?.language_code ?? "eng" }, set: { value in update(model) { $0.language_code = value } })) {
                    ForEach(Self.languages, id: \.0) { Text($0.1).tag($0.0) }
                }
                Toggle(isOn: Binding(get: { model.config?.spoken_commands_enabled ?? true }, set: { value in update(model) { $0.spoken_commands_enabled = value } })) {
                    Text("Spoken commands")
                    Text("“New line”, “new paragraph” and similar become formatting.")
                }
            }
        }
        .formStyle(.grouped)
    }

    private func saveKey() {
        do {
            try CoreBridge.shared.call("key.validate", ["key": .string(key)])
            try KeychainItem.elevenLabs.save(key)
            key = ""
            stored = true
            model.refreshKey()
            message = "Saved in your Keychain."
        } catch {
            message = error.localizedDescription
        }
    }
}

private struct ShortcutSettings: View {
    let model: AppModel
    @State private var error: String?

    var body: some View {
        Form {
            Section {
                keyPicker("Dictate", selection: model.config?.global_recording_key ?? "F9") { value in update(model) { $0.global_recording_key = value } }
                keyPicker("Agent session wheel", selection: model.agents.settings?.hold_key ?? "F12") { value in voice { $0.hold_key = value } }
                keyPicker("Keep draft", selection: model.agents.settings?.draft_key ?? "F7") { value in voice { $0.draft_key = value } }
                if let error { Text(error).font(.callout).foregroundStyle(.orange) }
                if !model.refusedKeys.isEmpty {
                    Text("macOS refused \(model.refusedKeys.map(\.rawValue).joined(separator: ", ")). Another app may own that key.")
                        .font(.callout).foregroundStyle(.orange)
                }
            } header: {
                Text("Global shortcuts")
            } footer: {
                Text("On a Mac keyboard, hold fn with the function key, or turn on “Use F1, F2, etc. keys as standard function keys” in Keyboard settings. Shortcuts see only their own key.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("While recording") {
                LabeledContent("Stop and deliver", value: "Press the same key again")
                LabeledContent("From the wheel", value: "Dictate records an editable draft")
            }
        }
        .formStyle(.grouped)
    }

    private func keyPicker(_ title: String, selection: String, set: @escaping (String) -> Void) -> some View {
        Picker(title, selection: Binding(get: { selection }, set: set)) {
            ForEach(FunctionKey.names, id: \.self) { Text($0).tag($0) }
        }
    }

    private func voice(_ change: (inout VoiceSettings) -> Void) {
        guard var settings = model.agents.settings else { return }
        change(&settings)
        do {
            try model.saveVoice(settings)
            error = nil
        } catch {
            self.error = error.localizedDescription
        }
    }
}

private struct AgentSettings: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                LabeledContent("Claude Code sessions") {
                    Text("Found from Claude Code's own session list").foregroundStyle(.secondary)
                }
                LabeledContent("Herdr") {
                    Text(model.agents.herdrReachable ? (model.agents.herdrGuarded ? "Connected · can send" : "Connected · this version can't send") : "Not running")
                        .foregroundStyle(.secondary)
                }
            } header: {
                Text("Discovery")
            } footer: {
                Text("Sending needs Herdr with the guarded prompt integration. Other sessions are draft only: Mluva records an editable transcript and never types into them.")
                    .font(.caption).foregroundStyle(.secondary)
            }
            Section("Sessions") {
                if model.agents.sessions.isEmpty {
                    Text("No running sessions.").foregroundStyle(.secondary)
                }
                ForEach(model.agents.sessions) { session in
                    HStack {
                        Circle().fill(AgentStatusPresentation.color(session.recipient.status.rawValue)).frame(width: 7, height: 7)
                        VStack(alignment: .leading) {
                            Text(session.display_name)
                            Text(session.draft_only ? "\(session.project) · draft only" : session.project)
                                .font(.caption).foregroundStyle(.secondary)
                        }
                        Spacer()
                        if model.agents.selected?.id == session.id {
                            Image(systemName: "checkmark").foregroundStyle(Color.mluva)
                        } else {
                            Button("Choose") { model.choose(session) }
                        }
                    }
                }
            }
        }
        .formStyle(.grouped)
        .task {
            while !Task.isCancelled {
                await model.agents.refresh()
                try? await Task.sleep(for: .seconds(2))
            }
        }
    }
}

private struct VocabularySettings: View {
    @State private var state = PersonalizationState(dictionary: [], snippets: [])
    @State private var spoken = ""
    @State private var written = ""
    @State private var error: String?

    var body: some View {
        Form {
            Section {
                ForEach(state.dictionary) { rule in
                    HStack {
                        Text(rule.spoken)
                        Image(systemName: "arrow.right").foregroundStyle(.secondary)
                        Text(rule.written).fontWeight(.medium)
                        Spacer()
                        Button("Remove", systemImage: "minus.circle") {
                            _ = try? CoreBridge.shared.call("personalization.delete_replacement", ["id": .string(rule.id)])
                            reload()
                        }
                        .labelStyle(.iconOnly)
                        .buttonStyle(.borderless)
                    }
                }
                HStack {
                    TextField("When I say", text: $spoken)
                    TextField("Write", text: $written)
                    Button("Add") {
                        do {
                            try CoreBridge.shared.call("personalization.save_replacement", ["spoken": .string(spoken), "written": .string(written)])
                            spoken = ""
                            written = ""
                            error = nil
                            reload()
                        } catch {
                            self.error = error.localizedDescription
                        }
                    }
                    .disabled(spoken.isEmpty || written.isEmpty)
                }
                if let error { Text(error).font(.callout).foregroundStyle(.orange) }
            } header: {
                Text("Replacements")
            } footer: {
                Text("Applied to every dictation and given to recognition as vocabulary. The original recognition is kept unchanged.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear(perform: reload)
    }

    private func reload() {
        if let loaded = try? CoreBridge.shared.call("personalization.state", as: PersonalizationState.self) {
            state = loaded
        }
    }
}

private struct PermissionSettings: View {
    let model: AppModel

    var body: some View {
        Form {
            Section {
                ForEach(Permission.allCases) { permission in
                    PermissionRow(permission: permission, permissions: model.permissions)
                }
            } footer: {
                Text("Mluva works with the microphone alone: dictations are copied. The other permissions add pasting and screen annotation.")
                    .font(.caption).foregroundStyle(.secondary)
            }
        }
        .formStyle(.grouped)
        .onAppear { model.permissions.refresh() }
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            model.permissions.refresh()
        }
    }
}

struct PermissionRow: View {
    let permission: Permission
    let permissions: PermissionCenter

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            Image(systemName: permission.symbol)
                .font(.title3)
                .foregroundStyle(Color.mluva)
                .frame(width: 26)
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 6) {
                    Text(permission.title).fontWeight(.medium)
                    if !permission.isRequired { Text("Optional").font(.caption).foregroundStyle(.secondary) }
                }
                Text(permission.purpose).font(.callout).foregroundStyle(.secondary).fixedSize(horizontal: false, vertical: true)
            }
            Spacer(minLength: 8)
            switch permissions.state(permission) {
            case .granted:
                Label("Allowed", systemImage: "checkmark.circle.fill")
                    .foregroundStyle(.green)
                    .labelStyle(.titleAndIcon)
            case .notAsked, .denied:
                Button(permissions.state(permission) == .denied ? "Open Settings" : "Allow…") { permissions.request(permission) }
            }
        }
        .padding(.vertical, 4)
    }
}
