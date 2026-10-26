import AppKit
import MluvaKit
import SwiftUI

/// The menu bar panel: status, Dictate, the agent session, the latest dictation and the way
/// to everything else.
struct MenuBarView: View {
    let model: AppModel
    @Environment(\.openWindow) private var openWindow
    @Environment(\.openSettings) private var openSettings

    private var dictation: DictationController { model.dictation }
    private var key: String { model.config?.global_recording_key ?? "F9" }
    private var agentKey: String { model.agents.settings?.hold_key ?? "F12" }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
                .padding(.horizontal, 16)
                .padding(.top, 14)
                .padding(.bottom, 12)
            if model.needsSetup {
                setupNotice
                    .padding(.horizontal, 16)
                    .padding(.bottom, 12)
            }
            Rule()
            VStack(spacing: 2) {
                KeyRow(key: key, title: dictation.phase == .recording ? "stop and copy" : "dictate",
                       value: dictation.phase == .recording ? "recording" : "copy to clipboard",
                       live: dictation.phase == .recording) {
                    model.stopOrStart(manual: true)
                }
                .disabled(model.needsSetup || dictation.phase == .finalizing)
                .help("Records and copies the text. Use \(key) anywhere to paste where you were typing.")
                KeyRow(key: agentKey, title: "agent wheel", value: model.agents.selected?.display_name ?? "\(model.agents.sessions.count) sessions") {
                    model.openWheel()
                }
                sessionPicker
            }
            .padding(.vertical, 6)
            .padding(.horizontal, 8)
            if let entry = model.recent.first {
                Rule()
                latest(entry)
                    .padding(.horizontal, 16)
                    .padding(.vertical, 12)
            }
            Rule()
            footer
                .padding(.horizontal, 16)
                .padding(.vertical, 10)
        }
        .frame(width: 340)
        .onAppear {
            model.permissions.refresh()
            model.refreshKey()
            model.refreshHistory()
            Task { await model.agents.refresh() }
        }
    }

    private var header: some View {
        HStack(spacing: 10) {
            JellyLight(mood: dictation.phase == .recording ? .listening : .idle, level: dictation.level, size: 22)
            Text("mluva").font(.mono(15, .bold))
            Spacer()
            if model.config?.incognito_mode == true {
                StateWord(text: "incognito", color: .secondary)
                    .help("Incognito: nothing is saved to History")
            }
            StateWord(text: status, color: dictation.phase == .recording ? .mluva : (model.needsSetup ? Ink.amber : .secondary))
        }
    }

    private var status: String {
        switch dictation.phase {
        case .recording: "rec"
        case .preparing: "arming"
        case .finalizing: "reading"
        default: model.needsSetup ? "setup" : "ready"
        }
    }

    private var setupNotice: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Text(!model.hasKey ? "Add your ElevenLabs key to start dictating." : "Allow the microphone to start dictating.")
                .font(.mono(11.5))
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            Button("Set up") { openWindow(id: "welcome"); NSApp.activate() }
                .controlSize(.small)
        }
    }

    /// The session the wheel and the agent key start on, chosen from a standard menu.
    private var sessionPicker: some View {
        let sessions = model.agents.sessions
        return Menu {
            if sessions.isEmpty { Text("No agent sessions found") }
            ForEach(sessions) { session in
                Button {
                    model.choose(session)
                } label: {
                    Text(session.draft_only ? "\(session.display_name) — draft only" : session.display_name)
                }
            }
        } label: {
            Text("choose session…").font(.mono(11))
        }
        .menuStyle(.borderlessButton)
        .menuIndicator(.hidden)
        .fixedSize()
        .foregroundStyle(.secondary)
        .padding(.leading, 64)
        .padding(.vertical, 2)
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func latest(_ entry: HistoryEntry) -> some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                StateWord(text: "last", color: .secondary)
                Spacer()
                if let date = entry.createdAt {
                    Text(date, format: .relative(presentation: .named))
                        .font(.mono(10.5))
                        .foregroundStyle(.secondary)
                }
            }
            Text(entry.delivered_text)
                .font(.mono(12))
                .lineSpacing(4)
                .lineLimit(3)
                .textSelection(.enabled)
            HStack(spacing: 14) {
                LinkAction(label: "copy") { TextDelivery.copy(entry.delivered_text) }
                LinkAction(label: "open in history") {
                    openWindow(id: "history", value: entry.identifier)
                    NSApp.activate()
                }
            }
        }
    }

    private var footer: some View {
        HStack(spacing: 16) {
            LinkAction(label: "history") { openWindow(id: "history", value: ""); NSApp.activate() }
            LinkAction(label: "settings") { openSettings(); NSApp.activate() }
            Spacer()
            LinkAction(label: "quit") { NSApp.terminate(nil) }
        }
    }
}

/// One command: its key on the left, what it does, and its current value on the right.
private struct KeyRow: View {
    let key: String
    let title: String
    let value: String
    var live = false
    let action: () -> Void
    @State private var hovering = false
    @Environment(\.isEnabled) private var enabled

    var body: some View {
        Button(action: action) {
            HStack(spacing: 12) {
                Keycap(key, lit: live)
                    .frame(width: 36, alignment: .leading)
                Text(title)
                    .font(.mono(12.5, .medium))
                Spacer(minLength: 8)
                Text(value)
                    .font(.mono(10.5))
                    .foregroundStyle(live ? Color.mluva : .secondary)
                    .lineLimit(1)
            }
            .padding(.horizontal, 8)
            .padding(.vertical, 7)
            .background(hovering && enabled ? Color.primary.opacity(0.07) : .clear, in: .rect(cornerRadius: 5))
            .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .opacity(enabled ? 1 : 0.45)
        .onHover { hovering = $0 }
    }
}

/// A lowercase word that acts, underlined on hover.
struct LinkAction: View {
    let label: String
    let action: () -> Void
    @State private var hovering = false

    var body: some View {
        Button(action: action) {
            Text(label)
                .font(.mono(11))
                .foregroundStyle(hovering ? .primary : .secondary)
                .underline(hovering)
                .contentShape(.rect)
        }
        .buttonStyle(.plain)
        .onHover { hovering = $0 }
    }
}
