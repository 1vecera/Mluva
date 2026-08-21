import AppKit
import SwiftUI
import UniformTypeIdentifiers

struct MeetingHistoryView: View {
    @EnvironmentObject var store: MeetingStore
    @State private var expandedMeetingIDs: Set<UUID> = []
    @State private var renamingMeetingID: UUID?
    @State private var proposedTitle = ""
    @State private var showsRenameAlert = false
    @State private var actionError: String?

    var body: some View {
        VStack(spacing: 0) {
            if store.meetings.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "person.2.wave.2")
                        .font(.title2)
                        .foregroundStyle(.secondary)
                    Text("No meetings yet")
                        .font(.subheadline)
                        .foregroundStyle(.secondary)
                    Text("Select Meeting and start an explicit capture.")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(store.meetings) { meeting in
                        meetingRow(meeting)
                            .contentShape(Rectangle())
                            .onTapGesture { toggleExpanded(meeting.id) }
                            .contextMenu { meetingMenu(meeting) }
                    }
                    .onDelete(perform: store.delete)
                }

                HStack {
                    Text("\(store.meetings.count) meetings")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    Spacer()
                    Button("Clear All") { store.clear() }
                        .font(.caption)
                        .foregroundStyle(.red)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
            }

            if let actionError {
                Text(actionError)
                    .font(.caption2)
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 6)
            }

            if let persistenceError = store.persistenceError {
                Label(
                    persistenceError,
                    systemImage: "externaldrive.badge.exclamationmark"
                )
                .font(.caption2)
                .foregroundStyle(.red)
                .lineLimit(2)
                .padding(.horizontal, 12)
                .padding(.bottom, 6)
            }
        }
        .frame(height: 300)
        .alert("Rename meeting", isPresented: $showsRenameAlert) {
            TextField("Title", text: $proposedTitle)
            Button("Cancel", role: .cancel) {}
            Button("Save") { commitRename() }
        }
    }

    private func meetingRow(_ meeting: MeetingRecord) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(meetingTitle(meeting))
                .font(.caption.bold())
                .lineLimit(1)

            if !meeting.insights.summary.isEmpty {
                Text(meeting.insights.summary)
                    .font(.caption)
                    .lineLimit(expandedMeetingIDs.contains(meeting.id) ? nil : 3)
                    .textSelection(.enabled)
            }

            HStack(spacing: 5) {
                Label(meeting.provider.displayName, systemImage: providerIcon(meeting.provider))
                Text(meeting.language)
                Text(durationLabel(meeting.duration))
                Spacer(minLength: 2)
                Text(meeting.timestamp, style: .relative)
            }
            .font(.caption2)
            .foregroundStyle(.secondary)

            Label(
                meeting.audioSources.map(audioSourceName).joined(separator: " + "),
                systemImage: "waveform"
            )
            .font(.caption2)
            .foregroundStyle(.secondary)

            if expandedMeetingIDs.contains(meeting.id) {
                meetingDetails(meeting)
            }
        }
        .padding(.vertical, 3)
    }

    @ViewBuilder
    private func meetingDetails(_ meeting: MeetingRecord) -> some View {
        Divider()
        insightSection("Decisions", values: meeting.insights.decisions)
        insightSection("Action items", values: meeting.insights.actionItems)

        Text("Speakers")
            .font(.caption2.bold())
            .foregroundStyle(.secondary)
        if meeting.speakers.isEmpty {
            Text("Speaker labels unavailable for this provider.")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            ForEach(Array(meeting.speakers.enumerated()), id: \.offset) { _, segment in
                Text("\(segment.speaker): \(segment.text)")
                    .font(.caption)
                    .textSelection(.enabled)
            }
        }

        Text("Transcript")
            .font(.caption2.bold())
            .foregroundStyle(.secondary)
        Text(meeting.transcript.isEmpty ? "No speech recognized." : meeting.transcript)
            .font(.caption)
            .textSelection(.enabled)
    }

    @ViewBuilder
    private func insightSection(_ title: String, values: [String]) -> some View {
        Text(title)
            .font(.caption2.bold())
            .foregroundStyle(.secondary)
        if values.isEmpty {
            Text("None recorded.")
                .font(.caption)
                .foregroundStyle(.secondary)
        } else {
            ForEach(Array(values.enumerated()), id: \.offset) { _, value in
                Text("• \(value)")
                    .font(.caption)
                    .textSelection(.enabled)
            }
        }
    }

    @ViewBuilder
    private func meetingMenu(_ meeting: MeetingRecord) -> some View {
        Button("Copy transcript") { copy(meeting.transcript) }
        Button("Copy summary") { copy(meeting.insights.summary) }
            .disabled(meeting.insights.summary.isEmpty)
        Button("Open recording") { openRecording(meeting) }
            .disabled(store.recordingURL(for: meeting) == nil)
        Divider()
        Button("Rename…") { beginRename(meeting) }
        Menu("Export") {
            Button("Markdown…") { export(meeting, format: .markdown) }
            Button("JSON…") { export(meeting, format: .json) }
        }
    }

    private func meetingTitle(_ meeting: MeetingRecord) -> String {
        meeting.title ?? meeting.timestamp.formatted(date: .abbreviated, time: .shortened)
    }

    private func durationLabel(_ duration: TimeInterval) -> String {
        let seconds = max(0, Int(duration.rounded()))
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }

    private func audioSourceName(_ source: MeetingAudioSource) -> String {
        switch source {
        case .microphone: "Microphone"
        case .system: "System audio"
        }
    }

    private func providerIcon(_ provider: TranscriptionProviderKind) -> String {
        switch provider {
        case .automatic: "waveform"
        case .apple: "apple.logo"
        case .googleCloud: "cloud"
        }
    }

    private func toggleExpanded(_ id: UUID) {
        if expandedMeetingIDs.contains(id) {
            expandedMeetingIDs.remove(id)
        } else {
            expandedMeetingIDs.insert(id)
        }
    }

    private func copy(_ text: String) {
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
        actionError = nil
    }

    private func openRecording(_ meeting: MeetingRecord) {
        guard let url = store.recordingURL(for: meeting) else {
            actionError = "The retained meeting recording is unavailable."
            return
        }
        NSWorkspace.shared.open(url)
        actionError = nil
    }

    private func beginRename(_ meeting: MeetingRecord) {
        renamingMeetingID = meeting.id
        proposedTitle = meeting.title ?? ""
        showsRenameAlert = true
    }

    private func commitRename() {
        guard let renamingMeetingID,
              let meeting = store.meetings.first(where: { $0.id == renamingMeetingID })
        else {
            return
        }
        store.save(meeting.renamed(proposedTitle))
        self.renamingMeetingID = nil
        proposedTitle = ""
    }

    private func export(
        _ meeting: MeetingRecord,
        format: TranscriptionExportFormat
    ) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(meetingTitle(meeting)).\(format.fileExtension)"
        panel.allowedContentTypes = switch format {
        case .markdown: [UTType(filenameExtension: "md") ?? .plainText]
        case .json: [.json]
        }
        panel.canCreateDirectories = true
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                try store.export(meeting: meeting, format: format)
                    .write(to: url, options: .atomic)
                actionError = nil
            } catch {
                actionError = error.localizedDescription
            }
        }
    }
}
