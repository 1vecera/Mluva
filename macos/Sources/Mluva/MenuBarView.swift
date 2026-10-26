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
        VStack(alignment: .leading, spacing: 14) {
            header
            if model.needsSetup { setupCallout }
            dictateButton
            agentRow
            if let latest = model.recent.first { latestCard(latest) }
            Divider()
            footer
        }
        .padding(16)
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
            JellyLight(mood: dictation.phase == .recording ? .listening : .idle, level: dictation.level, size: 26)
            VStack(alignment: .leading, spacing: 0) {
                Text("Mluva").font(.headline)
                Text(status).font(.caption).foregroundStyle(.secondary)
            }
            Spacer()
            if model.config?.incognito_mode == true {
                Label("Incognito", systemImage: "eye.slash")
                    .labelStyle(.iconOnly)
                    .foregroundStyle(.secondary)
                    .help("Incognito: nothing is saved to History")
            }
        }
    }

    private var status: String {
        switch dictation.phase {
        case .recording: "Listening"
        case .preparing: "Getting ready"
        case .finalizing: "Finishing"
        default: model.needsSetup ? "Setup needed" : "Ready · \(key) to dictate"
        }
    }

    private var setupCallout: some View {
        HStack(alignment: .firstTextBaseline, spacing: 10) {
            Image(systemName: "exclamationmark.circle.fill").foregroundStyle(.orange)
            Text(!model.hasKey ? "Add your ElevenLabs key to start dictating." : "Allow the microphone to start dictating.")
                .font(.callout)
                .fixedSize(horizontal: false, vertical: true)
            Spacer(minLength: 4)
            Button("Set up") { openWindow(id: "welcome"); NSApp.activate() }
                .controlSize(.small)
        }
        .padding(10)
        .background(.orange.opacity(0.12), in: .rect(cornerRadius: 10))
    }

    private var dictateButton: some View {
        Button {
            model.stopOrStart(manual: true)
        } label: {
            HStack {
                Image(systemName: dictation.phase == .recording ? "stop.fill" : "mic.fill")
                Text(dictation.phase == .recording ? "Stop and copy" : "Dictate")
                Spacer()
                Text(key).font(.callout.monospaced()).opacity(0.8)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 4)
        }
        .buttonStyle(.glassProminent)
        .tint(.mluva)
        .controlSize(.large)
        .disabled(model.needsSetup || dictation.phase == .finalizing)
        .help("Records and copies the text. Use \(key) anywhere to paste where you were typing.")
    }

    @ViewBuilder private var agentRow: some View {
        let sessions = model.agents.sessions
        HStack(spacing: 8) {
            Menu {
                if sessions.isEmpty {
                    Text("No agent sessions found")
                }
                ForEach(sessions) { session in
                    Button {
                        model.choose(session)
                    } label: {
                        Text(session.draft_only ? "\(session.display_name) — draft only" : session.display_name)
                    }
                }
            } label: {
                Label(model.agents.selected?.display_name ?? "Choose an agent session", systemImage: "terminal")
                    .lineLimit(1)
            }
            .menuStyle(.button)
            .buttonStyle(.glass)
            Button {
                model.openWheel()
            } label: {
                Text(agentKey).font(.callout.monospaced())
            }
            .buttonStyle(.glass)
            .help("Open the session wheel (\(agentKey))")
        }
    }

    private func latestCard(_ entry: HistoryEntry) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Text("Latest").font(.caption.weight(.semibold)).foregroundStyle(.secondary)
                Spacer()
                if let date = entry.createdAt {
                    Text(date, format: .relative(presentation: .named)).font(.caption).foregroundStyle(.secondary)
                }
            }
            Text(entry.delivered_text)
                .font(.callout)
                .lineLimit(3)
                .textSelection(.enabled)
            HStack {
                Button("Copy", systemImage: "doc.on.doc") { TextDelivery.copy(entry.delivered_text) }
                Button("Open", systemImage: "arrow.up.forward.app") {
                    openWindow(id: "history", value: entry.identifier)
                    NSApp.activate()
                }
            }
            .buttonStyle(.borderless)
            .font(.caption)
        }
        .padding(12)
        .background(.background.secondary, in: .rect(cornerRadius: 12))
    }

    private var footer: some View {
        HStack(spacing: 14) {
            Button("History") { openWindow(id: "history", value: ""); NSApp.activate() }
            Button("Settings…") { openSettings(); NSApp.activate() }
            Spacer()
            Button("Quit") { NSApp.terminate(nil) }
        }
        .buttonStyle(.borderless)
        .font(.callout)
    }
}
