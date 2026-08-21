import SwiftUI
import UniformTypeIdentifiers

struct HistoryView: View {
    @EnvironmentObject var appDelegate: AppDelegate
    @EnvironmentObject var store: TranscriptionStore
    @State private var expandedEntryIDs: Set<UUID> = []
    @State private var showsRenameAlert = false
    @State private var renamingEntryID: UUID?
    @State private var proposedTitle = ""
    @State private var editingEntry: TranscriptionEntry?
    @State private var correctedText = ""

    var body: some View {
        VStack(spacing: 0) {
            if store.entries.isEmpty {
                VStack(spacing: 8) {
                    Image(systemName: "doc.text")
                        .font(.title2)
                        .foregroundColor(.secondary)
                    Text("No transcriptions yet")
                        .font(.subheadline)
                        .foregroundColor(.secondary)
                }
                .frame(maxWidth: .infinity, maxHeight: .infinity)
            } else {
                List {
                    ForEach(store.entries, id: \.id) { entry in
                        VStack(alignment: .leading, spacing: 4) {
                            if let title = entry.title {
                                Text(title)
                                    .font(.caption.bold())
                                    .lineLimit(1)
                            }

                            Text(entry.deliveredText.isEmpty ? entry.rawText : entry.deliveredText)
                                .font(.caption)
                                .lineLimit(3)

                            HStack(spacing: 5) {
                                Label(providerName(entry.provider), systemImage: providerIcon(entry.provider))
                                Text(modeName(entry.mode))
                                Text(entry.language)
                                if let application = entry.targetApplicationName {
                                    Text(application)
                                }
                                Spacer(minLength: 2)
                                Text(entry.timestamp, style: .relative)
                            }
                            .font(.caption2)
                            .foregroundStyle(.secondary)

                            if entry.timings != .empty {
                                Label(
                                    timingLabel(entry.timings),
                                    systemImage: "gauge.with.dots.needle.33percent"
                                )
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            }

                            if let fallback = entry.fallbackEvent {
                                Label(
                                    fallbackLabel(fallback),
                                    systemImage: "arrow.trianglehead.swap"
                                )
                                .font(.caption2)
                                .foregroundStyle(.orange)
                            }

                            if let failureMessage = entry.failureMessage {
                                Label(
                                    failureMessage,
                                    systemImage: entry.deliveryOutcome == .failed
                                        ? "exclamationmark.triangle.fill"
                                        : "clock.badge.exclamationmark"
                                )
                                .font(.caption2)
                                .foregroundStyle(entry.deliveryOutcome == .failed ? .red : .orange)
                            }

                            if entry.retainedAudioFilename != nil {
                                Label("Recovery audio retained", systemImage: "waveform")
                                    .font(.caption2)
                                    .foregroundStyle(.secondary)
                            }

                            if entry.enhancementOutcome != .notRequested {
                                Label(
                                    enhancementLabel(entry.enhancementOutcome),
                                    systemImage: enhancementIcon(entry.enhancementOutcome)
                                )
                                .font(.caption2)
                                .foregroundStyle(enhancementColor(entry.enhancementOutcome))
                            }

                            if entry.styleOutcome != .notRequested {
                                Label(
                                    styleLabel(entry),
                                    systemImage: styleIcon(entry.styleOutcome)
                                )
                                .font(.caption2)
                                .foregroundStyle(styleColor(entry.styleOutcome))
                            }

                            if !entry.contextSources.isEmpty {
                                Label(
                                    contextLabel(entry.contextSources),
                                    systemImage: "rectangle.and.text.magnifyingglass"
                                )
                                .font(.caption2)
                                .foregroundStyle(.secondary)
                            }

                            if canRetryRecognition(entry) || canRetryDelivery(entry) {
                                HStack(spacing: 8) {
                                    if appDelegate.busyHistoryEntryIDs.contains(entry.id) {
                                        ProgressView()
                                            .controlSize(.small)
                                    } else {
                                        if canRetryRecognition(entry) {
                                            Button("Retry recognition") {
                                                appDelegate.retryRecognition(entry)
                                            }
                                            .buttonStyle(.bordered)
                                            .controlSize(.small)
                                        }

                                        if canRetryDelivery(entry) {
                                            Button(deliveryButtonTitle(entry)) {
                                                appDelegate.retryDelivery(entry)
                                            }
                                            .buttonStyle(.borderedProminent)
                                            .controlSize(.small)
                                        }
                                    }
                                }
                            }

                            if expandedEntryIDs.contains(entry.id),
                               entry.rawText != entry.deliveredText {
                                Divider()
                                Text("Raw transcript")
                                    .font(.caption2.bold())
                                    .foregroundStyle(.secondary)
                                Text(entry.rawText)
                                    .font(.caption)
                                    .textSelection(.enabled)
                            }
                        }
                        .contentShape(Rectangle())
                        .onTapGesture { toggleExpanded(entry.id) }
                        .contextMenu {
                            Button("Copy delivered text") {
                                copy(entry.deliveredText)
                            }
                            Button("Copy raw transcript") {
                                copy(entry.rawText)
                            }
                            Button(historyDeliveryActionTitle(entry)) {
                                appDelegate.retryDelivery(entry)
                            }
                            .disabled(!entry.canDeliverFromHistory)
                            Divider()
                            Button("Reprocess raw transcript") {
                                appDelegate.reprocess(entry)
                            }
                            .disabled(entry.rawText.isEmpty)
                            Button("Restore raw transcript") {
                                store.update(entry: entry.restoringRawTranscript())
                            }
                            .disabled(
                                entry.rawText.trimmingCharacters(
                                    in: .whitespacesAndNewlines
                                ).isEmpty || entry.rawText == entry.deliveredText
                            )
                            Button("Edit delivered text…") {
                                beginCorrection(entry)
                            }
                            .disabled(entry.deliveredText.isEmpty)
                            Button("Rename…") {
                                beginRename(entry)
                            }
                            Menu("Export") {
                                Button("Markdown…") {
                                    export(entry, format: .markdown)
                                }
                                Button("JSON…") {
                                    export(entry, format: .json)
                                }
                            }
                        }
                    }
                    .onDelete(perform: store.delete)
                }

                HStack {
                    Text("\(store.entries.count) entries")
                        .font(.caption2)
                        .foregroundColor(.secondary)
                    Spacer()
                    Button("Clear All") { store.clear() }
                        .font(.caption)
                        .foregroundColor(.red)
                }
                .padding(.horizontal, 12)
                .padding(.vertical, 6)
            }

            if let error = appDelegate.historyActionError {
                Text(error)
                    .font(.caption2)
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 6)
            }

            if let error = store.persistenceError {
                Label(error, systemImage: "externaldrive.badge.exclamationmark")
                    .font(.caption2)
                    .foregroundStyle(.red)
                    .lineLimit(2)
                    .padding(.horizontal, 12)
                    .padding(.bottom, 6)
            }
        }
        .frame(height: 260)
        .alert("Rename transcription", isPresented: $showsRenameAlert) {
            TextField("Title", text: $proposedTitle)
            Button("Cancel", role: .cancel) {}
            Button("Save") { commitRename() }
        }
        .sheet(item: $editingEntry) { entry in
            VStack(alignment: .leading, spacing: 12) {
                Text("Correct delivered text")
                    .font(.headline)
                Text("Small corrections can become review-only vocabulary suggestions. Nothing is added automatically.")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                TextEditor(text: $correctedText)
                    .font(.body)
                    .frame(minHeight: 140)
                    .padding(6)
                    .background(
                        RoundedRectangle(cornerRadius: 8)
                            .stroke(Color.secondary.opacity(0.3))
                    )
                HStack {
                    Spacer()
                    Button("Cancel") {
                        editingEntry = nil
                        correctedText = ""
                    }
                    Button("Save correction") {
                        commitCorrection(entry)
                    }
                    .buttonStyle(.borderedProminent)
                    .disabled(correctedText.trimmingCharacters(
                        in: .whitespacesAndNewlines
                    ).isEmpty)
                }
            }
            .padding(18)
            .frame(width: 420)
        }
    }

    private func toggleExpanded(_ id: UUID) {
        if expandedEntryIDs.contains(id) {
            expandedEntryIDs.remove(id)
        } else {
            expandedEntryIDs.insert(id)
        }
    }

    private func copy(_ text: String) {
        guard !text.isEmpty else { return }
        NSPasteboard.general.clearContents()
        NSPasteboard.general.setString(text, forType: .string)
    }

    private func beginRename(_ entry: TranscriptionEntry) {
        renamingEntryID = entry.id
        proposedTitle = entry.title ?? ""
        showsRenameAlert = true
    }

    private func commitRename() {
        guard let renamingEntryID,
              let entry = store.entries.first(where: { $0.id == renamingEntryID })
        else {
            return
        }
        store.update(entry: entry.renamed(proposedTitle))
        self.renamingEntryID = nil
        proposedTitle = ""
    }

    private func beginCorrection(_ entry: TranscriptionEntry) {
        correctedText = entry.deliveredText
        editingEntry = entry
    }

    private func commitCorrection(_ entry: TranscriptionEntry) {
        store.update(entry: entry.corrected(deliveredText: correctedText))
        editingEntry = nil
        correctedText = ""
    }

    private func export(
        _ entry: TranscriptionEntry,
        format: TranscriptionExportFormat
    ) {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(entry.title ?? "Mluva transcript").\(format.fileExtension)"
        panel.allowedContentTypes = switch format {
        case .markdown: [UTType(filenameExtension: "md") ?? .plainText]
        case .json: [.json]
        }
        panel.canCreateDirectories = true
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                let data = try store.export(entry: entry, format: format)
                try data.write(to: url, options: .atomic)
                appDelegate.historyActionError = nil
            } catch {
                appDelegate.historyActionError = error.localizedDescription
            }
        }
    }

    private func canRetryRecognition(_ entry: TranscriptionEntry) -> Bool {
        entry.deliveryOutcome == .failed && entry.retainedAudioFilename != nil
    }

    private func canRetryDelivery(_ entry: TranscriptionEntry) -> Bool {
        entry.deliveryOutcome == .pendingDelivery && entry.canDeliverFromHistory
    }

    private func historyDeliveryActionTitle(_ entry: TranscriptionEntry) -> String {
        let hasStoredTarget = entry.targetBundleIdentifier != nil
            || entry.targetApplicationName != nil
        return appDelegate.permissionsManager.accessibilityGranted && hasStoredTarget
            ? "Paste again"
            : "Copy for pasting"
    }

    private func deliveryButtonTitle(_ entry: TranscriptionEntry) -> String {
        let canPaste = appDelegate.permissionsManager.accessibilityGranted
            && (entry.targetBundleIdentifier != nil || entry.targetApplicationName != nil)
        return canPaste ? "Paste" : "Copy"
    }

    private func providerName(_ provider: TranscriptionProviderKind) -> String {
        switch provider {
        case .automatic: "Legacy"
        case .apple: "Apple"
        case .googleCloud: "Google"
        }
    }

    private func timingLabel(_ timings: TranscriptionTimings) -> String {
        [
            timingPart("Capture", timings.captureLatency),
            timingPart("Recognition", timings.recognitionLatency),
            timingPart("Enhancement", timings.enhancementLatency),
            timingPart("Delivery", timings.deliveryLatency),
        ]
        .compactMap { $0 }
        .joined(separator: " · ")
    }

    private func timingPart(_ label: String, _ duration: TimeInterval?) -> String? {
        duration.map { "\(label) \(Int(($0 * 1_000).rounded())) ms" }
    }

    private func fallbackLabel(_ fallback: ProviderFallbackEvent) -> String {
        let reason = switch fallback.reason {
        case .providerStartupFailed: "startup failed"
        case .providerFinalizationFailed: "final recognition failed"
        }
        return "\(providerName(fallback.from)) → \(providerName(fallback.to)): \(reason)"
    }

    private func modeName(_ mode: TranscriptionMode) -> String {
        switch mode {
        case .dictation: "Dictation"
        case .command: "Command"
        case .scratchpad: "Scratchpad"
        case .meeting: "Meeting"
        }
    }

    private func providerIcon(_ provider: TranscriptionProviderKind) -> String {
        switch provider {
        case .automatic: "waveform"
        case .apple: "apple.logo"
        case .googleCloud: "cloud"
        }
    }

    private func enhancementLabel(_ outcome: TranscriptEnhancementOutcome) -> String {
        switch outcome {
        case .notRequested: "Cleanup off"
        case .applied: "On-device cleanup applied"
        case .rejectedUnsafe: "Unsafe cleanup rejected"
        case .unavailable: "On-device cleanup unavailable"
        }
    }

    private func enhancementIcon(_ outcome: TranscriptEnhancementOutcome) -> String {
        switch outcome {
        case .notRequested: "text.badge.xmark"
        case .applied: "sparkles"
        case .rejectedUnsafe: "shield.lefthalf.filled"
        case .unavailable: "cpu"
        }
    }

    private func enhancementColor(_ outcome: TranscriptEnhancementOutcome) -> Color {
        outcome == .applied ? .secondary : .orange
    }

    private func styleLabel(_ entry: TranscriptionEntry) -> String {
        let name = entry.styleName ?? "Saved style"
        return switch entry.styleOutcome {
        case .notRequested: "Style off"
        case .applied: "\(name) applied"
        case .rejectedUnsafe: "Unsafe \(name) rewrite rejected"
        case .unavailable: "\(name) unavailable"
        }
    }

    private func styleIcon(_ outcome: TranscriptStyleOutcome) -> String {
        switch outcome {
        case .notRequested: "textformat"
        case .applied: "textformat.alt"
        case .rejectedUnsafe: "shield.lefthalf.filled"
        case .unavailable: "cpu"
        }
    }

    private func styleColor(_ outcome: TranscriptStyleOutcome) -> Color {
        outcome == .applied ? .secondary : .orange
    }

    private func contextLabel(_ sources: [TranscriptContextSource]) -> String {
        let names = sources.map(\.displayName).joined(separator: ", ")
        return "On-device context: \(names)"
    }
}
