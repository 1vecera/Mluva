import MluvaKit
import SwiftUI

/// First run: one thing per step, each saying what it enables and showing its live status.
struct OnboardingView: View {
    let model: AppModel
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var step = 0
    @State private var key = ""
    @State private var keyMessage: String?

    private let steps = ["Welcome", "Microphone", "Speech", "Pasting", "Shortcuts"]

    var body: some View {
        VStack(spacing: 0) {
            Group {
                switch step {
                case 0: welcome
                case 1: permission(.microphone, title: "Let Mluva hear you")
                case 2: speech
                case 3: permission(.accessibility, title: "Paste where you were typing")
                default: shortcuts
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity)
            .padding(.horizontal, 48)
            .transition(.opacity)
            footer
        }
        .frame(width: 600, height: 520)
        .animation(.smooth(duration: 0.25), value: step)
        .onReceive(NotificationCenter.default.publisher(for: NSApplication.didBecomeActiveNotification)) { _ in
            model.permissions.refresh()
        }
        .task {
            while !Task.isCancelled {
                model.permissions.refresh()
                try? await Task.sleep(for: .seconds(1))
            }
        }
    }

    private var welcome: some View {
        VStack(spacing: 18) {
            JellyLight(mood: .listening, level: 0.35, size: 120)
            Text("Speak. Mluva writes.")
                .font(.system(size: 34, weight: .semibold, design: .rounded))
            Text("Press a key anywhere, talk, press it again. Your words land in the app you were using, or in the agent session you chose.")
                .font(.title3)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 440)
        }
    }

    private func permission(_ permission: Permission, title: String) -> some View {
        VStack(spacing: 18) {
            Image(systemName: permission.symbol)
                .font(.system(size: 44, weight: .medium))
                .foregroundStyle(Color.mluva)
                .frame(height: 64)
            Text(title).font(.system(.title, design: .rounded).weight(.semibold))
            Text(permission.purpose)
                .font(.title3)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 440)
            switch model.permissions.state(permission) {
            case .granted:
                Label("\(permission.title) allowed", systemImage: "checkmark.circle.fill")
                    .font(.headline)
                    .foregroundStyle(.green)
            default:
                Button(permission == .microphone ? "Allow Microphone" : "Open \(permission.title) Settings") {
                    model.permissions.request(permission)
                }
                .buttonStyle(.glassProminent)
                .tint(.mluva)
                .controlSize(.large)
                if permission != .microphone {
                    Text("Switch on Mluva in the list, then come back. You can skip this: Mluva will copy instead.")
                        .font(.callout)
                        .foregroundStyle(.secondary)
                        .multilineTextAlignment(.center)
                }
            }
        }
    }

    private var speech: some View {
        VStack(spacing: 18) {
            Image(systemName: "waveform")
                .font(.system(size: 44, weight: .medium))
                .foregroundStyle(Color.mluva)
                .frame(height: 64)
            Text("Connect ElevenLabs Scribe").font(.system(.title, design: .rounded).weight(.semibold))
            Text("Mluva sends your audio to ElevenLabs while you dictate. Your key is kept in the Keychain.")
                .font(.title3)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .frame(maxWidth: 440)
            if model.hasKey {
                Label("Key saved in your Keychain", systemImage: "checkmark.circle.fill")
                    .font(.headline)
                    .foregroundStyle(.green)
            } else {
                HStack {
                    SecureField("ElevenLabs API key", text: $key)
                        .textFieldStyle(.roundedBorder)
                        .frame(width: 300)
                        .onSubmit(saveKey)
                    Button("Save", action: saveKey)
                        .buttonStyle(.glassProminent)
                        .tint(.mluva)
                        .disabled(key.isEmpty)
                }
                if let keyMessage { Text(keyMessage).font(.callout).foregroundStyle(.orange) }
            }
        }
    }

    private var shortcuts: some View {
        let dictate = model.config?.global_recording_key ?? "F9"
        let agent = model.agents.settings?.hold_key ?? "F12"
        return VStack(spacing: 22) {
            Text("Two keys to remember").font(.system(.title, design: .rounded).weight(.semibold))
            VStack(alignment: .leading, spacing: 16) {
                ShortcutLine(key: dictate, text: "Dictate. Press again to stop; the text is pasted or copied.")
                ShortcutLine(key: agent, text: "Choose an agent session, then press again to talk to it.")
                ShortcutLine(key: "fn", text: "Hold fn with the key on a Mac keyboard, unless function keys are standard.")
            }
            .frame(maxWidth: 440)
        }
    }

    private var footer: some View {
        HStack {
            HStack(spacing: 6) {
                ForEach(steps.indices, id: \.self) { index in
                    Capsule()
                        .fill(index == step ? Color.mluva : Color.secondary.opacity(0.3))
                        .frame(width: index == step ? 18 : 7, height: 7)
                }
            }
            .accessibilityElement()
            .accessibilityLabel("Step \(step + 1) of \(steps.count): \(steps[step])")
            Spacer()
            if step > 0 { Button("Back") { step -= 1 } }
            Button(step == steps.count - 1 ? "Start using Mluva" : "Continue") {
                if step == steps.count - 1 { complete() } else { step += 1 }
            }
            .buttonStyle(.glassProminent)
            .tint(.mluva)
            .keyboardShortcut(.defaultAction)
        }
        .padding(20)
    }

    private func saveKey() {
        do {
            try CoreBridge.shared.call("key.validate", ["key": .string(key)])
            try KeychainItem.elevenLabs.save(key)
            key = ""
            keyMessage = nil
            model.refreshKey()
        } catch {
            keyMessage = error.localizedDescription
        }
    }

    private func complete() {
        if var config = model.config {
            config.welcome_completed = true
            try? model.save(config)
        }
        dismissWindow(id: "welcome")
    }
}

private struct ShortcutLine: View {
    let key: String
    let text: String

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 14) {
            Text(key)
                .font(.system(.title3, design: .rounded).weight(.semibold))
                .frame(minWidth: 52)
                .padding(.vertical, 6)
                .glassEffect(.regular, in: .rect(cornerRadius: 10))
            Text(text).font(.title3)
        }
    }
}
