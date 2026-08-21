import SwiftUI

struct ScratchpadEditorView: View {
    let draft: ScratchpadDraft
    @Binding var text: String
    @Binding var selectedStyleID: UUID?
    let styles: [SavedStyle]
    let canInsert: Bool
    let isDelivering: Bool
    let isStyleWorking: Bool
    let onApplyStyle: () -> Void
    let onDelete: () -> Void
    let onCopy: () -> Void
    let onInsert: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 10) {
            HStack {
                Label("Scratchpad", systemImage: "note.text")
                    .font(.subheadline.bold())
                Spacer()
                if draft.entry.retainedAudioFilename != nil {
                    Label("Audio safe", systemImage: "waveform")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }
            }

            TextEditor(text: $text)
                .font(.body)
                .frame(minHeight: 120, maxHeight: 170)
                .padding(6)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.secondary.opacity(0.08))
                )
                .overlay(
                    RoundedRectangle(cornerRadius: 8)
                        .stroke(Color.secondary.opacity(0.16))
                )
                .disabled(isBusy)

            HStack(spacing: 8) {
                Picker("Writing style", selection: $selectedStyleID) {
                    Text("No style").tag(UUID?.none)
                    ForEach(styles) { style in
                        Text(style.name).tag(Optional(style.id))
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(maxWidth: .infinity, alignment: .leading)

                if isStyleWorking {
                    ProgressView()
                        .controlSize(.small)
                } else {
                    Button("Apply", action: onApplyStyle)
                        .buttonStyle(.bordered)
                        .disabled(selectedStyleID == nil)
                }
            }
            .disabled(isDelivering)

            if let appliedStyleName = draft.appliedStyleName {
                Label("Applied \(appliedStyleName)", systemImage: "sparkles")
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if draft.entry.rawText != text {
                DisclosureGroup("Raw transcript") {
                    Text(draft.entry.rawText)
                        .font(.caption)
                        .foregroundStyle(.secondary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
                .font(.caption2)
            }

            HStack {
                Button("Delete", role: .destructive, action: onDelete)
                    .buttonStyle(.bordered)

                Spacer()

                if isDelivering {
                    ProgressView()
                        .controlSize(.small)
                }

                Button("Copy", action: onCopy)
                    .buttonStyle(.bordered)

                Button(insertTitle, action: onInsert)
                    .buttonStyle(.borderedProminent)
                    .disabled(!canInsert)
            }
            .disabled(isBusy)

            Text(recoveryMessage)
                .font(.caption2)
                .foregroundStyle(.tertiary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.accentColor.opacity(0.08))
        )
    }

    private var insertTitle: String {
        guard let applicationName = draft.entry.targetApplicationName else {
            return "Insert"
        }
        return "Insert into \(applicationName)"
    }

    private var isBusy: Bool {
        isDelivering || isStyleWorking
    }

    private var recoveryMessage: String {
        if draft.entry.retainedAudioFilename != nil {
            return "The raw transcript and recovery audio remain until you choose an action."
        }
        return "This draft stays in memory only; no recovery audio is retained."
    }
}
