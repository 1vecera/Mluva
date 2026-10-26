import MluvaKit
import SwiftUI

/// Saved dictations: search, edit with autosave, and the original recognition kept apart.
struct HistoryView: View {
    let model: AppModel
    @Binding var selection: String?
    @State private var query = ""
    @State private var results: [HistoryEntry] = []

    var body: some View {
        NavigationSplitView {
            List(selection: $selection) {
                ForEach(groups, id: \.title) { group in
                    Section {
                        ForEach(group.entries) { entry in
                            HistoryRow(entry: entry).tag(entry.identifier)
                        }
                    } header: {
                        Text(group.title.lowercased()).font(.mono(10.5, .bold)).tracking(1)
                    }
                }
            }
            .searchable(text: $query, placement: .sidebar, prompt: "Search dictations")
            .navigationSplitViewColumnWidth(min: 250, ideal: 290)
            .overlay {
                if results.isEmpty {
                    EmptySheet(key: query.isEmpty ? (model.config?.global_recording_key ?? "F9") : nil,
                               title: query.isEmpty ? "no dictations yet" : "no matches",
                               detail: query.isEmpty ? "Press the key anywhere and talk." : "Try other words.")
                }
            }
        } detail: {
            if let selection, let entry = results.first(where: { $0.identifier == selection }) ?? find(selection) {
                HistoryDetail(model: model, entry: entry) { reload() }
                    .id(entry.identifier)
            } else {
                EmptySheet(key: nil, title: "choose a dictation", detail: "Its text opens here for editing.")
            }
        }
        .navigationTitle("History")
        .onAppear(perform: reload)
        .onChange(of: query) { reload() }
        .onChange(of: model.recent) { reload() }
    }

    private func find(_ id: String) -> HistoryEntry? {
        try? CoreBridge.shared.call("history.find", ["id": .string(id)], as: HistoryEntry.self)
    }

    private func reload() {
        if query.trimmingCharacters(in: .whitespaces).isEmpty {
            results = (try? CoreBridge.shared.call("history.recent", ["limit": 500], as: [HistoryEntry].self)) ?? []
        } else {
            results = (try? CoreBridge.shared.call("history.search", ["query": .string(query), "limit": 200], as: [HistoryEntry].self)) ?? []
        }
    }

    private var groups: [(title: String, entries: [HistoryEntry])] {
        let calendar = Calendar.current
        var order: [String] = []
        var grouped: [String: [HistoryEntry]] = [:]
        for entry in results {
            let date = entry.createdAt ?? .distantPast
            let title = calendar.isDateInToday(date) ? "Today"
                : calendar.isDateInYesterday(date) ? "Yesterday"
                : date.formatted(.dateTime.weekday(.wide).day().month(.wide))
            if grouped[title] == nil { order.append(title) }
            grouped[title, default: []].append(entry)
        }
        return order.map { ($0, grouped[$0]!) }
    }
}

/// A title line, then its outcome and the time set right against the margin.
private struct HistoryRow: View {
    let entry: HistoryEntry

    var body: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(entry.displayTitle)
                .font(.mono(12))
                .lineLimit(1)
            HStack(spacing: 6) {
                Text(outcome)
                Spacer(minLength: 4)
                if let date = entry.createdAt {
                    Text(date, format: .dateTime.hour(.twoDigits(amPM: .omitted)).minute())
                        .monospacedDigit()
                }
            }
            .font(.mono(10))
            .foregroundStyle(.secondary)
        }
        .padding(.vertical, 3)
    }

    private var outcome: String {
        switch entry.delivery_outcome {
        case "pasted": "pasted"
        case "paste-unconfirmed": "pasted · unconfirmed"
        case "copied": "copied"
        case "failed": "needs attention"
        // What became of an agent dictation, in the Linux app's words.
        case "sent": "sent"
        case "queued": "queued"
        case "delivered": "taken by the agent"
        case "prefilled": "in claude"
        case "held": "held for approval"
        case "unconfirmed": "sent · unconfirmed"
        case "busy": "draft · thread busy"
        case "cancelled": "cancelled"
        case "recipient-unavailable": "draft · session gone"
        default: "draft"
        }
    }
}

private struct HistoryDetail: View {
    let model: AppModel
    let entry: HistoryEntry
    let changed: () -> Void

    @State private var text = ""
    @State private var title = ""
    @State private var saved = true
    @State private var saveError: String?
    @State private var showOriginal = false
    @State private var confirmDelete = false
    @State private var pending: Task<Void, Never>?
    @State private var bundles: [Annotation.Bundle] = []

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            TextField("Title", text: $title, prompt: Text(entry.displayTitle))
                .textFieldStyle(.plain)
                .font(.mono(18, .bold))
                .padding(.horizontal, 24)
                .padding(.top, 18)
                .onSubmit(saveTitle)
            HStack(spacing: 8) {
                if let date = entry.createdAt { Text(date, format: .dateTime.weekday().day().month().hour().minute()) }
                Text("·")
                Text(saveError ?? (saved ? "saved" : "saving…"))
                    .foregroundStyle(saveError == nil ? Color.secondary : Ink.amber)
            }
            .font(.mono(10.5))
            .foregroundStyle(.secondary)
            .padding(.horizontal, 24)
            .padding(.vertical, 6)
            Rule()
                .padding(.horizontal, 24)
                .padding(.bottom, 10)
            // A fixed reading measure, about seventy characters, like the Linux editor.
            TextEditor(text: $text)
                .font(.mono(14))
                .lineSpacing(8)
                .scrollContentBackground(.hidden)
                .frame(maxWidth: 640, alignment: .leading)
                .frame(maxWidth: .infinity, alignment: .leading)
                .padding(.horizontal, 19)
                .onChange(of: text) { scheduleSave() }
            if !bundles.isEmpty {
                AnnotationStrip(bundles: bundles)
                    .padding(.horizontal, 24)
                    .padding(.top, 10)
            }
            DisclosureGroup(isExpanded: $showOriginal) {
                Text(entry.raw_text)
                    .font(.mono(12))
                    .lineSpacing(5)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.top, 6)
            } label: {
                Text("original recognition").font(.mono(11, .medium))
            }
            .padding(.horizontal, 24)
            .padding(.vertical, 14)
        }
        .toolbar {
            ToolbarItemGroup {
                Button("Copy", systemImage: "doc.on.doc") { TextDelivery.copy(text) }
                    .help("Copy the edited text")
                Button("Export", systemImage: "square.and.arrow.up") { export() }
                    .help("Save as Markdown")
                Button("Delete", systemImage: "trash", role: .destructive) { confirmDelete = true }
                    .help("Delete this dictation")
            }
        }
        .confirmationDialog("Delete this dictation?", isPresented: $confirmDelete) {
            Button("Delete", role: .destructive) {
                _ = try? CoreBridge.shared.call("history.delete", ["id": .string(entry.identifier)])
                model.refreshHistory()
                changed()
            }
        } message: {
            Text("Its text, original recognition and any retained audio are removed.")
        }
        .onAppear {
            text = (try? CoreBridge.shared.call("history.source", ["id": .string(entry.identifier)]).string) ?? entry.delivered_text
            // The automatic title reads as a title, not a placeholder; it is kept automatic until edited.
            title = entry.title ?? entry.displayTitle
            saved = true
            bundles = model.annotation.bundles(for: entry.identifier)
        }
        .onDisappear {
            flush()
            saveTitle()
        }
    }

    private func scheduleSave() {
        saved = false
        pending?.cancel()
        pending = Task {
            try? await Task.sleep(for: .milliseconds(600))
            guard !Task.isCancelled else { return }
            flush()
        }
    }

    private func flush() {
        guard !saved else { return }
        do {
            try CoreBridge.shared.call("history.save_text", ["id": .string(entry.identifier), "text": .string(text)])
            saved = true
            saveError = nil
        } catch {
            saveError = "Not saved yet: \(error.localizedDescription) Retrying."
            pending = Task {
                try? await Task.sleep(for: .seconds(2))
                flush()
            }
        }
    }

    private func saveTitle() {
        let trimmed = title.trimmingCharacters(in: .whitespacesAndNewlines)
        // Unchanged, or emptied back to automatic when it already is: nothing to save.
        guard trimmed != (entry.title ?? entry.displayTitle), !(trimmed.isEmpty && entry.title == nil) else { return }
        _ = try? CoreBridge.shared.call("history.title", ["id": .string(entry.identifier), "title": .string(trimmed)])
        changed()
    }

    private func export() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "\(entry.displayTitle).md"
        panel.canCreateDirectories = true
        guard panel.runModal() == .OK, let url = panel.url else { return }
        flush()
        let directory = FileManager.default.temporaryDirectory.appending(path: "mluva-export-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        try? FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        if let path = try? CoreBridge.shared.call("history.export", [
            "id": .string(entry.identifier), "directory": .string(directory.path), "format": "markdown",
        ]).string {
            try? FileManager.default.removeItem(at: url)
            try? FileManager.default.copyItem(at: URL(fileURLWithPath: path), to: url)
        }
    }
}

/// A conversation's screen annotation: its keyframes in order, each opening in Preview.
private struct AnnotationStrip: View {
    let bundles: [Annotation.Bundle]

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            HStack {
                Text("screen marks").font(.mono(11, .bold))
                Text(count).font(.mono(11)).foregroundStyle(.secondary)
                Spacer()
                LinkAction(label: "show in finder") {
                    NSWorkspace.shared.activateFileViewerSelecting(bundles.map { URL(fileURLWithPath: $0.contact_sheet) })
                }
            }
            ScrollView(.horizontal) {
                HStack(spacing: 10) {
                    ForEach(bundles.flatMap(\.keyframes), id: \.self) { path in
                        Button {
                            NSWorkspace.shared.open(URL(fileURLWithPath: path))
                        } label: {
                            Thumbnail(path: path)
                        }
                        .buttonStyle(.plain)
                        .help("Open keyframe \(URL(fileURLWithPath: path).deletingPathExtension().lastPathComponent)")
                    }
                }
            }
            .scrollIndicators(.never)
        }
    }

    private var count: String {
        let frames = bundles.reduce(0) { $0 + $1.keyframes.count }
        return frames == 1 ? "1 keyframe" : "\(frames) keyframes"
    }
}

private struct Thumbnail: View {
    let path: String
    @State private var image: NSImage?

    var body: some View {
        Group {
            if let image {
                Image(nsImage: image).resizable().aspectRatio(contentMode: .fit)
            } else {
                Rectangle().fill(.quaternary)
            }
        }
        .frame(height: 112)
        .frame(maxWidth: 200)
        .clipShape(.rect(cornerRadius: 3))
        .overlay(RoundedRectangle(cornerRadius: 3).strokeBorder(Color.primary.opacity(0.25), lineWidth: 0.75))
        .task(id: path) {
            let url = URL(fileURLWithPath: path)
            image = await Task.detached { NSImage(contentsOf: url) }.value
        }
    }
}

/// An empty state in the drafting language: the key that fills it, a line, and what to do.
private struct EmptySheet: View {
    let key: String?
    let title: String
    let detail: String

    var body: some View {
        VStack(spacing: 10) {
            if let key { Keycap(key, size: 14) }
            Text(title).font(.mono(13, .bold))
            Rule().frame(width: 80)
            Text(detail).font(.mono(11)).foregroundStyle(.secondary)
        }
        .multilineTextAlignment(.center)
        .padding(24)
    }
}
