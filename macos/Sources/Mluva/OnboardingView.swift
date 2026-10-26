import MluvaKit
import SwiftUI

/// First run: one thing per step, each saying what it enables and showing its live status.
struct OnboardingView: View {
    let model: AppModel
    @Environment(\.dismissWindow) private var dismissWindow
    @State private var step = 0
    @State private var key = ""
    @State private var keyMessage: String?

    private let steps = ["welcome", "microphone", "speech", "pasting", "keys"]

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            ruler
                .padding(.horizontal, 40)
                .padding(.top, 34)
            Group {
                switch step {
                case 0: welcome
                case 1: permission(.microphone, title: "let mluva hear you")
                case 2: speech
                case 3: permission(.accessibility, title: "paste where you were typing")
                default: shortcuts
                }
            }
            .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
            .padding(.horizontal, 40)
            .padding(.top, 40)
            .transition(.opacity)
            Rule()
            footer
        }
        .frame(width: 640, height: 500)
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

    /// The steps as a ruled line of names, the current one underlined in red.
    private var ruler: some View {
        HStack(spacing: 0) {
            ForEach(steps.indices, id: \.self) { index in
                VStack(alignment: .leading, spacing: 6) {
                    Text(steps[index])
                        .font(.mono(10.5, index == step ? .bold : .regular))
                        .foregroundStyle(index <= step ? .primary : .secondary)
                    Rectangle()
                        .fill(index == step ? Color.mluva : (index < step ? Color.primary.opacity(0.5) : Color.primary.opacity(0.15)))
                        .frame(height: index == step ? 2 : 1)
                }
                .frame(maxWidth: .infinity, alignment: .leading)
            }
        }
        .accessibilityElement()
        .accessibilityLabel("Step \(step + 1) of \(steps.count): \(steps[step])")
    }

    private var welcome: some View {
        let key = model.config?.global_recording_key ?? "F9"
        return VStack(alignment: .leading, spacing: 22) {
            HStack(alignment: .center, spacing: 16) {
                JellyLight(mood: .listening, level: 0.35, size: 54)
                Text("speak.\nmluva writes.")
                    .font(.mono(30, .bold))
                    .lineSpacing(2)
            }
            Text("Press a key anywhere, talk, press it again. Your words land in the app you were using, or in the agent session you chose.")
                .font(.mono(13))
                .lineSpacing(6)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 500, alignment: .leading)
            // How it goes, drawn as a callout chain.
            HStack(spacing: 0) {
                Keycap(key, size: 13)
                Leader()
                Text("talk").font(.mono(12))
                Leader()
                Keycap(key, size: 13)
                Leader()
                Text("text lands where you were").font(.mono(12))
            }
            .padding(.top, 8)
        }
    }

    private func permission(_ permission: Permission, title: String) -> some View {
        VStack(alignment: .leading, spacing: 16) {
            Text(title).font(.mono(22, .bold))
            Text(permission.purpose)
                .font(.mono(13))
                .lineSpacing(6)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 500, alignment: .leading)
            switch model.permissions.state(permission) {
            case .granted:
                HStack(spacing: 8) {
                    StatusMark(status: .idle, size: 8)
                    Text("\(permission.title.lowercased()) allowed").font(.mono(12, .medium))
                }
                .padding(.top, 6)
            default:
                Button(permission == .microphone ? "Allow Microphone" : "Open \(permission.title) Settings") {
                    model.permissions.request(permission)
                }
                .buttonStyle(.glassProminent)
                .tint(.mluva)
                .controlSize(.large)
                .padding(.top, 6)
                if permission != .microphone {
                    Text("Switch on Mluva in the list, then come back. You can skip this: Mluva will copy instead.")
                        .font(.mono(11))
                        .foregroundStyle(.secondary)
                }
            }
        }
    }

    private var speech: some View {
        VStack(alignment: .leading, spacing: 16) {
            Text("connect elevenlabs scribe").font(.mono(22, .bold))
            Text("Mluva streams your audio to ElevenLabs while you dictate. Your key stays in the Keychain.")
                .font(.mono(13))
                .lineSpacing(6)
                .foregroundStyle(.secondary)
                .frame(maxWidth: 500, alignment: .leading)
            if model.hasKey {
                HStack(spacing: 8) {
                    StatusMark(status: .idle, size: 8)
                    Text("key saved in your keychain").font(.mono(12, .medium))
                }
                .padding(.top, 6)
            } else {
                HStack {
                    SecureField("ElevenLabs API key", text: $key)
                        .textFieldStyle(.roundedBorder)
                        .font(.mono(12))
                        .frame(width: 320)
                        .onSubmit(saveKey)
                    Button("Save", action: saveKey)
                        .buttonStyle(.glassProminent)
                        .tint(.mluva)
                        .disabled(key.isEmpty)
                }
                .padding(.top, 6)
                if let keyMessage { Text(keyMessage).font(.mono(11)).foregroundStyle(Ink.amber) }
            }
        }
    }

    private var shortcuts: some View {
        let dictate = model.config?.global_recording_key ?? "F9"
        let agent = model.agents.settings?.hold_key ?? "F12"
        return VStack(alignment: .leading, spacing: 20) {
            Text("two keys to remember").font(.mono(22, .bold))
            VStack(alignment: .leading, spacing: 14) {
                ShortcutLine(key: dictate, text: "Dictate. Press again to stop; the text is pasted or copied.")
                ShortcutLine(key: agent, text: "Choose an agent session, then press again to talk to it.")
                ShortcutLine(key: "fn", text: "Hold fn with the key on a Mac keyboard, unless function keys are standard.")
            }
            .frame(maxWidth: 520, alignment: .leading)
        }
    }

    private var footer: some View {
        HStack {
            Spacer()
            if step > 0 { Button("Back") { step -= 1 } }
            Button(step == steps.count - 1 ? "Start using Mluva" : "Continue") {
                if step == steps.count - 1 { complete() } else { step += 1 }
            }
            .buttonStyle(.glassProminent)
            .tint(.mluva)
            .keyboardShortcut(.defaultAction)
        }
        .padding(.horizontal, 24)
        .padding(.vertical, 16)
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
        HStack(alignment: .firstTextBaseline, spacing: 16) {
            Keycap(key, size: 13)
                .frame(width: 48, alignment: .leading)
            Text(text).font(.mono(12.5)).lineSpacing(4)
        }
    }
}

/// A short hairline leader with an arrowhead, joining the steps of a callout chain.
private struct Leader: View {
    var body: some View {
        Canvas { context, area in
            let mid = area.height / 2
            var path = Path()
            path.move(to: CGPoint(x: 6, y: mid))
            path.addLine(to: CGPoint(x: area.width - 6, y: mid))
            path.move(to: CGPoint(x: area.width - 10, y: mid - 3))
            path.addLine(to: CGPoint(x: area.width - 6, y: mid))
            path.addLine(to: CGPoint(x: area.width - 10, y: mid + 3))
            context.stroke(path, with: .color(.secondary), lineWidth: 0.75)
        }
        .frame(width: 34, height: 12)
        .accessibilityHidden(true)
    }
}
