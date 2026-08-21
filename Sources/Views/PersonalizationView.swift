import SwiftUI

struct PersonalizationView: View {
    @EnvironmentObject private var store: PersonalizationStore
    @EnvironmentObject private var transcriptionStore: TranscriptionStore

    @State private var section: Section = .dictionary
    @State private var spoken = ""
    @State private var written = ""
    @State private var matchDictionaryCase = false
    @State private var trigger = ""
    @State private var typedTrigger = ""
    @State private var expansion = ""
    @State private var styleName = ""
    @State private var styleInstructions = ""
    @State private var applicationScopeEnabled = false
    @State private var scopeApplicationName: String?
    @State private var scopeBundleIdentifier: String?

    private enum Section: String, CaseIterable, Identifiable {
        case dictionary = "Dictionary"
        case snippets = "Snippets"
        case styles = "Styles"

        var id: Self { self }
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 12) {
            Picker("Personalization", selection: $section) {
                ForEach(Section.allCases) { section in
                    Text(section.rawValue).tag(section)
                }
            }
            .labelsHidden()
            .pickerStyle(.segmented)

            if let scopeApplicationName, scopeBundleIdentifier != nil {
                Toggle("Only in \(scopeApplicationName)", isOn: $applicationScopeEnabled)
                    .font(.caption)
                    .toggleStyle(.switch)
            }

            switch section {
            case .dictionary:
                dictionaryEditor
            case .snippets:
                snippetEditor
            case .styles:
                styleEditor
            }

            if let error = store.persistenceError {
                Label(error, systemImage: "exclamationmark.triangle.fill")
                    .font(.caption2)
                    .foregroundStyle(.red)
                    .lineLimit(2)
            }
        }
        .padding(16)
        .frame(height: 340, alignment: .top)
        .onAppear(perform: refreshApplicationScope)
    }

    private var dictionaryEditor: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Teach recognition how your names and technical terms should be written.")
                .font(.caption)
                .foregroundStyle(.secondary)

            Toggle("Match the capitalization pattern you spoke", isOn: $matchDictionaryCase)
                .font(.caption)
                .toggleStyle(.switch)

            HStack(spacing: 6) {
                TextField("What you say", text: $spoken)
                    .textFieldStyle(.roundedBorder)
                TextField("Written as", text: $written)
                    .textFieldStyle(.roundedBorder)
                Button {
                    store.saveDictionaryReplacement(
                        spoken: spoken,
                        written: written,
                        bundleIdentifier: activeScopeBundleIdentifier,
                        caseBehavior: matchDictionaryCase ? .matchSpoken : .fixed
                    )
                    spoken = ""
                    written = ""
                    matchDictionaryCase = false
                } label: {
                    Image(systemName: "plus")
                }
                .disabled(isBlank(spoken) || isBlank(written))
            }

            if store.dictionary.isEmpty && vocabularySuggestions.isEmpty {
                emptyState("No dictionary entries yet")
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        if !vocabularySuggestions.isEmpty {
                            Text("Suggestions from your edits")
                                .font(.caption2.bold())
                                .foregroundStyle(.secondary)
                                .frame(maxWidth: .infinity, alignment: .leading)
                            ForEach(vocabularySuggestions) { suggestion in
                                vocabularySuggestionRow(suggestion)
                            }
                            if !store.dictionary.isEmpty {
                                Divider()
                            }
                        }
                        ForEach(store.dictionary) { replacement in
                            personalizationRow(
                                source: replacement.spoken,
                                destination: replacement.written,
                                bundleIdentifier: replacement.bundleIdentifier,
                                detail: replacement.caseBehavior == .matchSpoken
                                    ? "Matches capitalization"
                                    : nil,
                                delete: { store.deleteDictionaryReplacement(id: replacement.id) }
                            )
                        }
                    }
                }
            }
        }
    }

    private var snippetEditor: some View {
        VStack(alignment: .leading, spacing: 10) {
            Text("Say “snippet” followed by a spoken trigger. Optionally add an exact one-token typed trigger. Variables: {{date}}, {{time}}, {{datetime}}, {{weekday}}.")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack(spacing: 6) {
                TextField("Spoken trigger", text: $trigger)
                    .textFieldStyle(.roundedBorder)
                TextField("Typed trigger (optional)", text: $typedTrigger)
                    .textFieldStyle(.roundedBorder)
            }
            HStack(alignment: .bottom, spacing: 6) {
                TextEditor(text: $expansion)
                    .font(.callout)
                    .scrollContentBackground(.hidden)
                    .padding(5)
                    .frame(height: 54)
                    .background(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color.secondary.opacity(0.3))
                    )
                Button {
                    store.saveSnippet(
                        trigger: trigger,
                        typedTrigger: typedTrigger,
                        expansion: expansion,
                        bundleIdentifier: activeScopeBundleIdentifier
                    )
                    trigger = ""
                    typedTrigger = ""
                    expansion = ""
                } label: {
                    Image(systemName: "plus")
                }
                .disabled(isBlank(trigger) || isBlank(expansion))
            }

            if store.snippets.isEmpty {
                emptyState("No snippets yet")
            } else {
                ScrollView {
                    LazyVStack(spacing: 6) {
                        ForEach(store.snippets) { snippet in
                            personalizationRow(
                                source: snippet.trigger,
                                destination: snippet.expansion,
                                bundleIdentifier: snippet.bundleIdentifier,
                                detail: snippet.typedTrigger.map { "Typed: \($0)" },
                                delete: { store.deleteSnippet(id: snippet.id) }
                            )
                        }
                    }
                }
            }
        }
    }

    private var styleEditor: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Apply a saved writing style before delivery, or explicitly from Scratchpad.")
                .font(.caption)
                .foregroundStyle(.secondary)

            TextField("Style name", text: $styleName)
                .textFieldStyle(.roundedBorder)

            HStack(alignment: .bottom, spacing: 6) {
                TextEditor(text: $styleInstructions)
                    .font(.callout)
                    .scrollContentBackground(.hidden)
                    .padding(5)
                    .frame(height: 54)
                    .background(
                        RoundedRectangle(cornerRadius: 6)
                            .stroke(Color.secondary.opacity(0.3))
                    )
                Button {
                    store.saveStyle(
                        name: styleName,
                        instructions: styleInstructions
                    )
                    styleName = ""
                    styleInstructions = ""
                } label: {
                    Image(systemName: "plus")
                }
                .disabled(isBlank(styleName) || isBlank(styleInstructions))
            }

            ScrollView {
                LazyVStack(spacing: 6) {
                    ForEach(store.styles) { style in
                        styleRow(style)
                    }
                }
            }
        }
    }

    private func styleRow(_ style: SavedStyle) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                HStack(spacing: 5) {
                    Text(style.name)
                        .font(.caption.bold())
                    if style.isBuiltIn {
                        Text("Built-in")
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
                Text(style.instructions)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
            Spacer(minLength: 4)
            if !style.isBuiltIn {
                Button {
                    store.deleteStyle(id: style.id)
                } label: {
                    Image(systemName: "trash")
                        .foregroundStyle(.secondary)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Delete \(style.name)")
            }
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(Color.secondary.opacity(0.08))
        )
    }

    private func vocabularySuggestionRow(
        _ suggestion: VocabularySuggestion
    ) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text("\(suggestion.spoken) → \(suggestion.written)")
                    .font(.caption.bold())
                HStack(spacing: 5) {
                    Text(suggestion.occurrences == 1
                        ? "One correction"
                        : "\(suggestion.occurrences) corrections")
                    if let bundleIdentifier = suggestion.bundleIdentifier {
                        Label(bundleIdentifier, systemImage: "app.badge")
                    }
                }
                .font(.caption2)
                .foregroundStyle(.tertiary)
            }
            Spacer(minLength: 4)
            Button {
                store.saveDictionaryReplacement(
                    spoken: suggestion.spoken,
                    written: suggestion.written,
                    bundleIdentifier: suggestion.bundleIdentifier
                )
            } label: {
                Image(systemName: "plus.circle.fill")
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Add \(suggestion.written) to dictionary")
            Button {
                store.dismissVocabularySuggestion(id: suggestion.id)
            } label: {
                Image(systemName: "xmark")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Dismiss suggestion for \(suggestion.written)")
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(Color.orange.opacity(0.08))
        )
    }

    private func personalizationRow(
        source: String,
        destination: String,
        bundleIdentifier: String?,
        detail: String? = nil,
        delete: @escaping () -> Void
    ) -> some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            VStack(alignment: .leading, spacing: 2) {
                Text(source)
                    .font(.caption.bold())
                Text(destination)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                if let detail {
                    Text(detail)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                }
                if let bundleIdentifier {
                    Label(bundleIdentifier, systemImage: "app.badge")
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }
            Spacer(minLength: 4)
            Button(action: delete) {
                Image(systemName: "trash")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .accessibilityLabel("Delete \(source)")
        }
        .padding(8)
        .background(
            RoundedRectangle(cornerRadius: 7)
                .fill(Color.secondary.opacity(0.08))
        )
    }

    private func emptyState(_ message: String) -> some View {
        Text(message)
            .font(.caption)
            .foregroundStyle(.tertiary)
            .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private func isBlank(_ value: String) -> Bool {
        value.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    private var activeScopeBundleIdentifier: String? {
        applicationScopeEnabled ? scopeBundleIdentifier : nil
    }

    private var vocabularySuggestions: [VocabularySuggestion] {
        VocabularySuggestionEngine().suggestions(
            from: transcriptionStore.entries,
            dictionary: store.dictionary,
            dismissedIDs: store.dismissedVocabularySuggestionIDs
        )
    }

    private func refreshApplicationScope() {
        let target = ApplicationFocusTracker.shared.captureTarget()
        scopeApplicationName = target?.targetApplicationName
        scopeBundleIdentifier = target?.targetBundleIdentifier
        if scopeBundleIdentifier == nil {
            applicationScopeEnabled = false
        }
    }
}
