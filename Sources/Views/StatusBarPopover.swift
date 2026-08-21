import SwiftUI
import AppKit
import CoreGraphics
import UniformTypeIdentifiers

private struct CaptureModePresentation: Identifiable {
    let mode: TranscriptionMode
    let title: String
    let explanation: String

    var id: String { mode.rawValue }

    static let all = [
        CaptureModePresentation(
            mode: .dictation,
            title: "Dictate",
            explanation: "Transcribe your speech, clean it up, and insert it into the active app."
        ),
        CaptureModePresentation(
            mode: .command,
            title: "Command",
            explanation: "Transform selected text and review before applying. Uses Gemini 3.6 Flash first, with Apple Intelligence as backup."
        ),
        CaptureModePresentation(
            mode: .scratchpad,
            title: "Notes",
            explanation: "Capture a longer thought in an editable draft before copying or inserting it."
        ),
        CaptureModePresentation(
            mode: .meeting,
            title: "Meeting",
            explanation: "Record microphone and system audio, then save a transcript and meeting notes."
        ),
    ]
}

struct StatusBarPopover: View {
    @EnvironmentObject var appDelegate: AppDelegate
    @EnvironmentObject var recordingController: RecordingController
    @EnvironmentObject var permissionsManager: PermissionsManager
    @EnvironmentObject var transcriptionStore: TranscriptionStore
    @EnvironmentObject var meetingController: MeetingController
    @EnvironmentObject var meetingStore: MeetingStore
    @EnvironmentObject var personalizationStore: PersonalizationStore

    @State private var currentView: PopoverView = .main
    @State private var captureMode = AppSettings.shared.transcriptionMode
    @State private var selectedProviderPreference = AppSettings.shared.providerPreference
    @State private var selectedStyleID: UUID?
    @State private var styleTargetApplicationName: String?
    @State private var styleTargetBundleIdentifier: String?
    @State private var diagnosticsExportError: String?
    @State private var googleServiceAccountFilePath = AppSettings.shared.googleServiceAccountFilePath
    @State private var googleCredentialError: String?
    @State private var isOutputModeEditorVisible = false
    @State private var editedOutputModeID: UUID?
    @State private var outputModeName = ""
    @State private var outputModeInstructions = ""

    enum PopoverView {
        case main, settings, history, meetings, personalization
    }

    var body: some View {
        VStack(spacing: 0) {
            // Header
            HStack {
                Text("Mluva")
                    .font(.headline)
                Spacer()
                if currentView == .main {
                    Button(action: { currentView = .settings }) {
                        Image(systemName: "gear")
                            .font(.system(size: 13))
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, 16)
            .padding(.top, 14)
            .padding(.bottom, 10)

            Divider()

            if !AppSettings.shared.hasCompletedSetup
                || permissionsManager.missingRecognitionRequirement() != nil {
                SetupView()
                    .environmentObject(permissionsManager)
            } else {
                switch currentView {
                case .main:     mainView
                case .settings: settingsView
                case .history:  HistoryView().environmentObject(transcriptionStore)
                case .meetings: MeetingHistoryView().environmentObject(meetingStore)
                case .personalization:
                    PersonalizationView().environmentObject(personalizationStore)
                }
            }

            Divider()

            // Footer
            HStack {
                if currentView != .main {
                    Button("Back") { currentView = .main }
                        .buttonStyle(.plain)
                        .font(.caption)
                } else if permissionsManager.canRecord {
                    Button("History") { currentView = .history }
                        .buttonStyle(.plain)
                        .font(.caption)
                    Button("Meetings") { currentView = .meetings }
                        .buttonStyle(.plain)
                        .font(.caption)
                    Button("Personalize") { currentView = .personalization }
                        .buttonStyle(.plain)
                        .font(.caption)
                }
                Spacer()
                Button("Quit") { NSApplication.shared.terminate(nil) }
                    .buttonStyle(.plain)
                    .font(.caption)
                    .foregroundColor(.secondary)
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 8)
        }
        .frame(width: 350)
    }

    // MARK: - Main View

    private var mainView: some View {
        VStack(spacing: 16) {
            Spacer().frame(height: 8)

            captureModePicker
            .onChange(of: captureMode) { _, mode in
                if mode == .meeting {
                    AppSettings.shared.transcriptionMode = .meeting
                } else if AppSettings.shared.rememberLastModePerApplication,
                   styleTargetBundleIdentifier != nil {
                    if AppSettings.shared.transcriptionMode == .meeting {
                        AppSettings.shared.transcriptionMode = mode
                    }
                    personalizationStore.selectMode(
                        mode,
                        for: styleTargetBundleIdentifier,
                        rememberPerApplication: true
                    )
                } else {
                    AppSettings.shared.transcriptionMode = mode
                }
            }
            .disabled(recordingController.state != .idle
                || meetingController.state != .idle
                || recordingController.pendingCommandPreview != nil
                || recordingController.pendingScratchpadDraft != nil)

            if captureMode != .command,
               captureMode != .meeting,
               recordingController.pendingScratchpadDraft == nil {
                writingStylePicker
            }

            // Accessibility hint banner
            if captureMode != .meeting,
               !permissionsManager.accessibilityGranted {
                accessibilityBanner
            }

            if captureMode == .meeting {
                meetingCaptureStatus
            } else if let preview = recordingController.pendingCommandPreview {
                commandPreview(preview)
            } else if let draft = recordingController.pendingScratchpadDraft {
                ScratchpadEditorView(
                    draft: draft,
                    text: Binding(
                        get: { recordingController.pendingScratchpadDraft?.text ?? "" },
                        set: { recordingController.updateScratchpadText($0) }
                    ),
                    selectedStyleID: Binding(
                        get: {
                            recordingController.pendingScratchpadDraft?.selectedStyleID
                        },
                        set: { recordingController.updateScratchpadStyle($0) }
                    ),
                    styles: personalizationStore.styles,
                    canInsert: permissionsManager.accessibilityGranted
                        && draft.entry.targetApplicationName != nil,
                    isDelivering: recordingController.isScratchpadAcceptanceInProgress,
                    isStyleWorking: recordingController.isScratchpadStyleProcessingInProgress,
                    onApplyStyle: { appDelegate.applyScratchpadStyle() },
                    onDelete: { appDelegate.discardScratchpadDraft() },
                    onCopy: { appDelegate.copyScratchpadDraft() },
                    onInsert: { appDelegate.insertScratchpadDraft() }
                )
            } else {
                ZStack {
                    Circle()
                        .fill(recordingController.state == .recording
                              ? Color.red.opacity(0.15)
                              : Color.secondary.opacity(0.08))
                        .frame(width: 80, height: 80)

                    if recordingController.state == .starting {
                        ProgressView()
                            .controlSize(.regular)
                    } else {
                        Image(systemName: recordingController.state == .recording ? "mic.fill" : "mic")
                            .font(.system(size: 32))
                            .foregroundColor(recordingController.state == .recording ? .red : .secondary)
                    }
                }

                Text(statusText)
                    .font(.subheadline)
                    .foregroundColor(.secondary)

                Text(providerDisplayName)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)

                if let fallbackNotice = recordingController.fallbackNotice {
                    Label(fallbackNotice, systemImage: "arrow.trianglehead.swap")
                        .font(.caption2)
                        .foregroundStyle(.orange)
                }
            }

            if captureMode != .meeting,
               recordingController.state == .recording,
               let startedAt = recordingController.recordingStartedAt {
                HStack(spacing: 8) {
                    AudioLevelBars(level: recordingController.audioLevel)
                    ElapsedRecordingTime(startedAt: startedAt)
                    if let inputDeviceName = recordingController.inputDeviceName {
                        Text(inputDeviceName)
                            .lineLimit(1)
                    }
                }
                .font(.caption2)
                .foregroundStyle(.secondary)
            }

            Toggle("Incognito", isOn: Binding(
                get: { AppSettings.shared.incognitoMode },
                set: { AppSettings.shared.incognitoMode = $0 }
            ))
            .font(.caption)
            .toggleStyle(.switch)
            .tint(.orange)
            .disabled(activeCaptureState != .idle)

            if !activePartialText.isEmpty {
                Text(activePartialText)
                    .font(.caption)
                    .foregroundColor(.secondary)
                    .lineLimit(2)
                    .frame(maxWidth: .infinity, alignment: .leading)
                    .padding(.horizontal, 4)
            }

            if let error = activeCaptureError {
                Text(error)
                    .font(.caption)
                    .foregroundColor(.red)
                    .lineLimit(2)
                    .padding(.horizontal, 4)
            }

            if recordingController.pendingCommandPreview == nil
                && recordingController.pendingScratchpadDraft == nil {
                Button(action: toggleActiveCapture) {
                    Text(activeButtonText)
                        .frame(maxWidth: .infinity)
                }
                .controlSize(.large)
                .buttonStyle(.borderedProminent)
                .tint(activeCaptureState == .recording ? .red : .accentColor)
                .disabled(captureButtonDisabled)
            }

            Text(captureMode == .meeting
                 ? "Explicit capture · microphone + system audio"
                 : "Hold \(hotkeyDisplay) · double-tap for hands-free · Esc cancels")
                .font(.caption2)
                .foregroundStyle(.tertiary)

            Spacer().frame(height: 4)
        }
        .padding(.horizontal, 16)
        .onAppear(perform: refreshApplicationProfileSelection)
    }

    private var captureModePicker: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text("Capture mode")
                .font(.caption)
                .foregroundStyle(.secondary)

            HStack(spacing: 2) {
                ForEach(CaptureModePresentation.all) { option in
                    let isSelected = captureMode == option.mode

                    Button {
                        captureMode = option.mode
                    } label: {
                        Text(option.title)
                            .font(.caption.weight(isSelected ? .semibold : .regular))
                            .lineLimit(1)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 6)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .foregroundStyle(isSelected ? Color.white : Color.primary)
                    .background(
                        RoundedRectangle(cornerRadius: 6, style: .continuous)
                            .fill(isSelected ? Color.accentColor : Color.clear)
                    )
                    .help(option.explanation)
                    .accessibilityLabel(option.title)
                    .accessibilityHint(option.explanation)
                    .accessibilityAddTraits(isSelected ? .isSelected : [])
                }
            }
            .padding(2)
            .background(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .fill(Color.secondary.opacity(0.12))
            )
            .overlay(
                RoundedRectangle(cornerRadius: 8, style: .continuous)
                    .stroke(Color.secondary.opacity(0.12), lineWidth: 1)
            )
            .accessibilityElement(children: .contain)
            .accessibilityLabel("Capture mode")
        }
    }

    private var meetingCaptureStatus: some View {
        VStack(spacing: 10) {
            if !permissionsManager.screenRecordingGranted {
                HStack(spacing: 7) {
                    Image(systemName: "rectangle.inset.filled.badge.record")
                        .foregroundStyle(.orange)
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Screen Recording permission required")
                            .font(.caption.bold())
                        Text("macOS uses it to expose system audio. Mluva does not capture video.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                    Spacer(minLength: 4)
                    Button("Grant") {
                        permissionsManager.requestScreenRecording()
                    }
                    .font(.caption2)
                    .controlSize(.mini)
                }
                .padding(10)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.orange.opacity(0.08))
                )
            }

            ZStack {
                Circle()
                    .fill(meetingController.state == .recording
                          ? Color.red.opacity(0.15)
                          : Color.secondary.opacity(0.08))
                    .frame(width: 80, height: 80)

                Image(systemName: meetingController.state == .recording
                      ? "person.2.wave.2.fill"
                      : "person.2.wave.2")
                    .font(.system(size: 30))
                    .foregroundStyle(
                        meetingController.state == .recording ? .red : .secondary
                    )
            }

            Text(meetingStatusText)
                .font(.subheadline)
                .foregroundStyle(.secondary)

            Text(meetingProviderDisplayName)
                .font(.caption2)
                .foregroundStyle(.tertiary)

            if meetingController.state == .recording,
               let startedAt = meetingController.recordingStartedAt {
                ElapsedRecordingTime(startedAt: startedAt)
                    .font(.caption2)
                    .foregroundStyle(.secondary)
            }

            if meetingController.state == .idle,
               meetingController.lastMeeting != nil {
                Label(
                    meetingController.lastMeeting?.recordingFilename == nil
                        ? "Meeting finished without saving"
                        : "Meeting saved to its private archive",
                    systemImage: meetingController.lastMeeting?.recordingFilename == nil
                        ? "eye.slash"
                        : "checkmark.circle.fill"
                )
                .font(.caption2)
                .foregroundStyle(.secondary)
            }

            if meetingController.state == .recording {
                Button("Cancel without saving", role: .destructive) {
                    appDelegate.cancelMeeting()
                }
                .buttonStyle(.plain)
                .font(.caption)
            }
        }
    }

    private var writingStylePicker: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(spacing: 8) {
                Label("Output mode", systemImage: "textformat")
                    .font(.caption)
                    .foregroundStyle(.secondary)

                Picker("Output mode", selection: $selectedStyleID) {
                    Text("Faithful cleanup")
                        .help("Preserves meaning while fixing punctuation, structure, and speech disfluency.")
                        .tag(UUID?.none)
                    ForEach(personalizationStore.styles) { style in
                        Text(style.name)
                            .help(style.instructions)
                            .tag(Optional(style.id))
                    }
                }
                .labelsHidden()
                .pickerStyle(.menu)
                .frame(maxWidth: .infinity, alignment: .trailing)
                .help(selectedOutputModeHelp)
                .onChange(of: selectedStyleID) { _, id in
                    personalizationStore.selectStyle(
                        id,
                        for: styleTargetBundleIdentifier,
                        rememberPerApplication: AppSettings.shared.rememberLastStylePerApplication
                    )
                    isOutputModeEditorVisible = false
                }

                Button {
                    beginNewOutputMode()
                } label: {
                    Label("New", systemImage: "plus")
                }
                .buttonStyle(.borderless)
                .help("Create output mode")
            }

            Text(selectedOutputMode?.instructions ?? "Faithful cleanup only: preserve meaning while fixing punctuation, structure, and speech disfluency.")
                .font(.caption2)
                .foregroundStyle(.secondary)
                .fixedSize(horizontal: false, vertical: true)
                .textSelection(.enabled)

            HStack(spacing: 10) {
                if let selectedOutputMode,
                   !selectedOutputMode.isBuiltIn {
                    Button("Edit") {
                        beginEditingOutputMode(selectedOutputMode)
                    }
                    .buttonStyle(.plain)
                    .font(.caption2)

                    Button("Delete", role: .destructive) {
                        personalizationStore.deleteStyle(id: selectedOutputMode.id)
                        selectedStyleID = nil
                    }
                    .buttonStyle(.plain)
                    .font(.caption2)
                }

                if AppSettings.shared.rememberLastStylePerApplication,
                   let styleTargetApplicationName {
                    Spacer()
                    Text(styleTargetApplicationName)
                        .font(.caption2)
                        .foregroundStyle(.tertiary)
                        .lineLimit(1)
                }
            }

            if isOutputModeEditorVisible {
                VStack(alignment: .leading, spacing: 6) {
                    TextField("Mode name", text: $outputModeName)
                        .textFieldStyle(.roundedBorder)
                    TextEditor(text: $outputModeInstructions)
                        .font(.caption)
                        .frame(minHeight: 72, maxHeight: 96)
                        .overlay(
                            RoundedRectangle(cornerRadius: 5)
                                .stroke(Color.secondary.opacity(0.25))
                        )
                    HStack {
                        Button("Cancel") {
                            isOutputModeEditorVisible = false
                        }
                        .buttonStyle(.plain)
                        Spacer()
                        Button(editedOutputModeID == nil ? "Create mode" : "Save mode") {
                            saveOutputMode()
                        }
                        .buttonStyle(.borderedProminent)
                        .controlSize(.small)
                        .disabled(
                            outputModeName.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                                || outputModeInstructions.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
                        )
                    }
                }
                .padding(9)
                .background(
                    RoundedRectangle(cornerRadius: 8)
                        .fill(Color.secondary.opacity(0.06))
                )
            }
        }
        .disabled(recordingController.state != .idle)
    }

    private var selectedOutputMode: SavedStyle? {
        personalizationStore.style(id: selectedStyleID)
    }

    private var selectedOutputModeHelp: String {
        if let selectedOutputMode {
            return "\(selectedOutputMode.name): \(selectedOutputMode.instructions)"
        }
        return "Faithful cleanup: preserves meaning while fixing punctuation, structure, and speech disfluency."
    }

    private func beginNewOutputMode() {
        editedOutputModeID = nil
        outputModeName = ""
        outputModeInstructions = ""
        isOutputModeEditorVisible = true
    }

    private func beginEditingOutputMode(_ mode: SavedStyle) {
        guard !mode.isBuiltIn else { return }
        editedOutputModeID = mode.id
        outputModeName = mode.name
        outputModeInstructions = mode.instructions
        isOutputModeEditorVisible = true
    }

    private func saveOutputMode() {
        let savedID: UUID?
        if let editedOutputModeID {
            savedID = personalizationStore.updateStyle(
                id: editedOutputModeID,
                name: outputModeName,
                instructions: outputModeInstructions
            ) ? editedOutputModeID : nil
        } else {
            savedID = personalizationStore.saveStyle(
                name: outputModeName,
                instructions: outputModeInstructions
            )
        }
        guard let savedID else { return }
        selectedStyleID = savedID
        personalizationStore.selectStyle(
            savedID,
            for: styleTargetBundleIdentifier,
            rememberPerApplication: AppSettings.shared.rememberLastStylePerApplication
        )
        isOutputModeEditorVisible = false
    }

    private func refreshApplicationProfileSelection() {
        let target = ApplicationFocusTracker.shared.captureTarget()
        styleTargetApplicationName = target?.targetApplicationName
        styleTargetBundleIdentifier = target?.targetBundleIdentifier
        let globalMode = AppSettings.shared.transcriptionMode
        if globalMode == .meeting {
            captureMode = .meeting
            selectedProviderPreference = AppSettings.shared.providerPreference
        } else {
            let selectedMode = personalizationStore.selectedMode(
                for: styleTargetBundleIdentifier,
                rememberPerApplication: AppSettings.shared.rememberLastModePerApplication,
                fallback: globalMode
            )
            captureMode = selectedMode == .meeting ? globalMode : selectedMode
            selectedProviderPreference = personalizationStore.selectedProvider(
                for: styleTargetBundleIdentifier,
                rememberPerApplication: AppSettings.shared.rememberProviderPerApplication,
                fallback: AppSettings.shared.providerPreference
            )
        }
        selectedStyleID = personalizationStore.selectedStyle(
            for: styleTargetBundleIdentifier,
            rememberPerApplication: AppSettings.shared.rememberLastStylePerApplication
        )?.id
    }

    private func commandPreview(_ preview: TranscriptCommandPreview) -> some View {
        VStack(alignment: .leading, spacing: 10) {
            Label("Review command", systemImage: "text.badge.checkmark")
                .font(.subheadline.bold())

            if let sourceText = preview.sourceText {
                VStack(alignment: .leading, spacing: 3) {
                    Text("Selected text")
                        .font(.caption2.bold())
                        .foregroundStyle(.secondary)
                    Text(sourceText)
                        .font(.caption)
                        .lineLimit(3)
                }

            }

            VStack(alignment: .leading, spacing: 3) {
                Text("Proposed result")
                    .font(.caption2.bold())
                    .foregroundStyle(.secondary)
                Text(preview.proposedText)
                    .font(.callout)
                    .lineLimit(6)
                    .textSelection(.enabled)
            }

            HStack {
                Button("Discard") {
                    appDelegate.discardCommandPreview()
                }
                .buttonStyle(.bordered)

                Button(preview.sourceText == nil ? "Insert" : "Replace selection") {
                    appDelegate.acceptCommandPreview()
                }
                .buttonStyle(.borderedProminent)
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
        .padding(12)
        .background(
            RoundedRectangle(cornerRadius: 10)
                .fill(Color.accentColor.opacity(0.08))
        )
    }

    // MARK: - Accessibility Banner

    private var accessibilityBanner: some View {
        HStack(spacing: 6) {
            Image(systemName: "exclamationmark.triangle.fill")
                .font(.caption)
                .foregroundColor(.orange)
            Text("Grant Accessibility for global hotkey & auto-paste")
                .font(.caption2)
                .foregroundColor(.secondary)
                .lineLimit(2)
            Spacer(minLength: 0)
            Button("Grant") {
                permissionsManager.requestAccessibility()
            }
            .font(.caption2)
            .buttonStyle(.bordered)
            .controlSize(.mini)
        }
        .padding(.horizontal, 10)
        .padding(.vertical, 6)
        .background(
            RoundedRectangle(cornerRadius: 6)
                .fill(Color.orange.opacity(0.08))
        )
    }

    // MARK: - Settings View

    private var settingsView: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 16) {
                Group {
                    Text("Recognition").font(.caption.bold()).foregroundColor(.secondary)
                    Picker("Provider", selection: $selectedProviderPreference) {
                        Text("Automatic").tag(TranscriptionProviderKind.automatic)
                        Text("Apple").tag(TranscriptionProviderKind.apple)
                        Text("Google").tag(TranscriptionProviderKind.googleCloud)
                    }
                    .labelsHidden()
                    .pickerStyle(.segmented)
                    .onChange(of: selectedProviderPreference) { _, provider in
                        if captureMode != .meeting,
                           AppSettings.shared.rememberProviderPerApplication,
                           styleTargetBundleIdentifier != nil {
                            personalizationStore.selectProvider(
                                provider,
                                for: styleTargetBundleIdentifier,
                                rememberPerApplication: true
                            )
                        } else {
                            AppSettings.shared.providerPreference = provider
                        }
                    }

                    if AppSettings.shared.rememberProviderPerApplication,
                       let styleTargetApplicationName {
                        Text("Provider applies to \(styleTargetApplicationName)")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }

                    Toggle("Require on-device Apple Speech", isOn: Binding(
                        get: { AppSettings.shared.requiresOnDeviceAppleSpeech },
                        set: { AppSettings.shared.requiresOnDeviceAppleSpeech = $0 }
                    ))
                    .font(.callout)

                    Toggle("Allow Google Cloud processing", isOn: Binding(
                        get: { AppSettings.shared.cloudRecognitionAllowed },
                        set: { AppSettings.shared.cloudRecognitionAllowed = $0 }
                    ))
                    .font(.callout)

                    Toggle("Prefer Google for technical speech", isOn: Binding(
                        get: { AppSettings.shared.preferCloudForTechnicalSpeech },
                        set: { AppSettings.shared.preferCloudForTechnicalSpeech = $0 }
                    ))
                    .font(.callout)

                    TextField("Google Cloud project ID", text: Binding(
                        get: { AppSettings.shared.googleCloudProjectID },
                        set: { AppSettings.shared.googleCloudProjectID = $0 }
                    ))
                    .textFieldStyle(.roundedBorder)

                    VStack(alignment: .leading, spacing: 5) {
                        Text("Google authentication")
                            .font(.caption.bold())
                            .foregroundStyle(.secondary)
                        HStack {
                            Text(googleAuthenticationLabel)
                                .font(.caption)
                                .lineLimit(1)
                                .truncationMode(.middle)
                            Spacer()
                            Button("Choose JSON…") {
                                chooseGoogleServiceAccountFile()
                            }
                            .font(.caption)
                            if !googleServiceAccountFilePath.isEmpty {
                                Button("Use ADC") {
                                    googleServiceAccountFilePath = ""
                                    AppSettings.shared.googleServiceAccountFilePath = ""
                                    googleCredentialError = nil
                                }
                                .font(.caption)
                            }
                        }
                        Text("The selected file stays in place. Mluva stores only its path and reads the private key in memory when Google needs a token.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                        if let googleCredentialError {
                            Text(googleCredentialError)
                                .font(.caption2)
                                .foregroundStyle(.red)
                        }
                    }

                    Picker("Google region", selection: Binding(
                        get: { AppSettings.shared.googleCloudLocation },
                        set: { AppSettings.shared.googleCloudLocation = $0 }
                    )) {
                        Text("EU").tag("eu")
                        Text("US").tag("us")
                    }
                    .pickerStyle(.segmented)
                }

                Divider()

                // Hotkey Configuration
                hotkeySettingsSection

                Divider()

                // Language
                Group {
                    Text("Language").font(.caption.bold()).foregroundColor(.secondary)
                    Picker("", selection: Binding(
                        get: { AppSettings.shared.language },
                        set: { AppSettings.shared.language = $0 }
                    )) {
                        Text("Auto-detect").tag("auto")
                        Text("English").tag("en")
                        Text("Czech").tag("cs")
                        Text("Spanish").tag("es")
                        Text("French").tag("fr")
                        Text("German").tag("de")
                        Text("Italian").tag("it")
                        Text("Portuguese").tag("pt")
                        Text("Dutch").tag("nl")
                        Text("Japanese").tag("ja")
                        Text("Chinese").tag("zh")
                        Text("Korean").tag("ko")
                        Text("Polish").tag("pl")
                        Text("Russian").tag("ru")
                    }
                    .labelsHidden()
                }

                Toggle("Remove filler words", isOn: Binding(
                    get: { AppSettings.shared.removeFiller },
                    set: { AppSettings.shared.removeFiller = $0 }
                ))
                .font(.callout)

                VStack(alignment: .leading, spacing: 3) {
                    Toggle("Expand typed snippet triggers", isOn: Binding(
                        get: { AppSettings.shared.typedSnippetExpansionEnabled },
                        set: { AppSettings.shared.typedSnippetExpansionEnabled = $0 }
                    ))
                    .font(.callout)
                    Text("Keeps only the current typed token in memory, never stores it, and stays off in secure text fields. Requires Accessibility.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                VStack(alignment: .leading, spacing: 3) {
                    Toggle("Faithful cleanup", isOn: Binding(
                        get: { AppSettings.shared.faithfulEnhancementEnabled },
                        set: { AppSettings.shared.faithfulEnhancementEnabled = $0 }
                    ))
                    .font(.callout)
                    Text("When Google Cloud processing is enabled, Command uses Gemini 3.6 Flash first and Apple Intelligence as backup. Gemini receives only the spoken instruction and selected text—never window or nearby context. Incognito remains local-only.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                Toggle("Remember style per application", isOn: Binding(
                    get: { AppSettings.shared.rememberLastStylePerApplication },
                    set: {
                        AppSettings.shared.rememberLastStylePerApplication = $0
                        refreshApplicationProfileSelection()
                    }
                ))
                .font(.callout)

                Toggle("Remember mode per application", isOn: Binding(
                    get: { AppSettings.shared.rememberLastModePerApplication },
                    set: {
                        AppSettings.shared.rememberLastModePerApplication = $0
                        refreshApplicationProfileSelection()
                    }
                ))
                .font(.callout)

                Toggle("Remember provider per application", isOn: Binding(
                    get: { AppSettings.shared.rememberProviderPerApplication },
                    set: {
                        AppSettings.shared.rememberProviderPerApplication = $0
                        refreshApplicationProfileSelection()
                    }
                ))
                .font(.callout)

                VStack(alignment: .leading, spacing: 3) {
                    Toggle("Use on-device application context", isOn: Binding(
                        get: { AppSettings.shared.contextualFormattingEnabled },
                        set: { AppSettings.shared.contextualFormattingEnabled = $0 }
                    ))
                    .font(.callout)
                    Text("Window titles, selected text, and nearby accessible text can guide Apple Intelligence. This context is never sent to Google.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                }

                if let styleTargetBundleIdentifier,
                   let styleTargetApplicationName {
                    Toggle("Allow context in \(styleTargetApplicationName)", isOn: Binding(
                        get: {
                            AppSettings.shared.contextualFormattingAllowed(
                                for: styleTargetBundleIdentifier
                            )
                        },
                        set: {
                            AppSettings.shared.setContextualFormatting(
                                $0,
                                for: styleTargetBundleIdentifier
                            )
                        }
                    ))
                    .font(.callout)
                    .disabled(!AppSettings.shared.contextualFormattingEnabled)
                }

                Picker("Keep audio", selection: Binding(
                    get: { AppSettings.shared.audioRetentionPolicy },
                    set: { AppSettings.shared.audioRetentionPolicy = $0 }
                )) {
                    Text("Never").tag(AudioRetentionPolicy.never)
                    Text("Failures").tag(AudioRetentionPolicy.failures)
                    Text("Always").tag(AudioRetentionPolicy.always)
                }
                .font(.callout)

                Picker("Keep history", selection: Binding(
                    get: { AppSettings.shared.historyRetentionDays },
                    set: {
                        AppSettings.shared.historyRetentionDays = $0
                        transcriptionStore.updateRetention(days: $0)
                    }
                )) {
                    Text("Forever").tag(0)
                    Text("7 days").tag(7)
                    Text("30 days").tag(30)
                    Text("90 days").tag(90)
                }
                .font(.callout)

                VStack(alignment: .leading, spacing: 3) {
                    Button("Export privacy-safe diagnostics…") {
                        exportDiagnostics()
                    }
                    .font(.callout)
                    Text("Includes configuration, outcomes, and timings; excludes transcripts, audio, application names, and project ID.")
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                    if let diagnosticsExportError {
                        Text(diagnosticsExportError)
                            .font(.caption2)
                            .foregroundStyle(.red)
                    }
                }

                Divider()

                // Permissions status
                Group {
                    Text("Permissions").font(.caption.bold()).foregroundColor(.secondary)
                    HStack {
                        Text("Microphone")
                        Spacer()
                        Image(systemName: permissionsManager.microphoneGranted
                              ? "checkmark.circle.fill" : "xmark.circle")
                            .foregroundColor(permissionsManager.microphoneGranted ? .green : .red)
                    }
                    .font(.callout)
                    HStack {
                        Text("Apple Speech")
                        Spacer()
                        Image(systemName: permissionsManager.speechRecognitionGranted
                              ? "checkmark.circle.fill" : "xmark.circle")
                            .foregroundColor(permissionsManager.speechRecognitionGranted ? .green : .red)
                    }
                    .font(.callout)
                    HStack {
                        Text("Accessibility")
                        Spacer()
                        if permissionsManager.accessibilityGranted {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundColor(.green)
                        } else {
                            HStack(spacing: 4) {
                                Text("Optional")
                                    .font(.caption2)
                                    .foregroundColor(.orange)
                                Image(systemName: "minus.circle")
                                    .foregroundColor(.orange)
                            }
                        }
                    }
                    .font(.callout)
                    HStack {
                        Text("Screen Recording")
                        Spacer()
                        if permissionsManager.screenRecordingGranted {
                            Image(systemName: "checkmark.circle.fill")
                                .foregroundStyle(.green)
                        } else {
                            Button("Meeting only") {
                                permissionsManager.requestScreenRecording()
                            }
                            .font(.caption2)
                            .controlSize(.mini)
                        }
                    }
                    .font(.callout)

                    Button("Refresh") { permissionsManager.checkAll() }
                        .font(.caption)
                }
            }
            .padding(16)
        }
        .frame(height: 340)
    }

    // MARK: - Hotkey Settings Section

    @State private var isRecordingHotkey = false
    @State private var hotkeyDisplay: String = AppSettings.shared.hotkeyDisplayString

    private var hotkeySettingsSection: some View {
        VStack(alignment: .leading, spacing: 8) {
            Text("Global Hotkey").font(.caption.bold()).foregroundColor(.secondary)

            HStack {
                Text("Current:")
                    .font(.callout)
                Text(hotkeyDisplay)
                    .font(.callout.monospaced())
                    .padding(.horizontal, 8)
                    .padding(.vertical, 3)
                    .background(
                        RoundedRectangle(cornerRadius: 4)
                            .fill(Color.secondary.opacity(0.12))
                    )
                Spacer()
            }

            HStack(spacing: 8) {
                if isRecordingHotkey {
                    HotkeyRecorderView(
                        isRecording: $isRecordingHotkey,
                        onCapture: { keyCode, modifiers in
                            appDelegate.updateHotkey(keyCode: keyCode, modifiers: modifiers)
                            hotkeyDisplay = AppSettings.shared.hotkeyDisplayString
                            isRecordingHotkey = false
                        }
                    )
                    .frame(height: 24)

                    Button("Cancel") {
                        isRecordingHotkey = false
                    }
                    .font(.caption)
                    .buttonStyle(.plain)
                    .foregroundColor(.secondary)
                } else {
                    Button("Record New Shortcut") {
                        isRecordingHotkey = true
                    }
                    .font(.caption)

                    Button("Reset to Default") {
                        AppSettings.shared.resetHotkeyToDefault()
                        appDelegate.updateHotkey(
                            keyCode: AppSettings.defaultHotkeyKeyCode,
                            modifiers: CGEventFlags(rawValue: AppSettings.defaultHotkeyModifiers)
                        )
                        hotkeyDisplay = AppSettings.shared.hotkeyDisplayString
                    }
                    .font(.caption)
                    .foregroundColor(.secondary)
                }
            }

            if !permissionsManager.accessibilityGranted {
                Text("Requires Accessibility permission to function")
                    .font(.caption2)
                    .foregroundColor(.orange)
            }
        }
    }

    // MARK: - Helpers

    private var activeCaptureState: RecordingState {
        captureMode == .meeting ? meetingController.state : recordingController.state
    }

    private var activePartialText: String {
        captureMode == .meeting ? meetingController.partialText : recordingController.partialText
    }

    private var activeCaptureError: String? {
        captureMode == .meeting ? meetingController.error : recordingController.error
    }

    private var activeButtonText: String {
        if captureMode != .meeting {
            return buttonText
        }
        return switch meetingController.state {
        case .idle: "Start Meeting"
        case .starting: "Connecting…"
        case .recording: "Stop and Save"
        case .stopping: "Saving…"
        }
    }

    private var captureButtonDisabled: Bool {
        if activeCaptureState == .starting || activeCaptureState == .stopping {
            return true
        }
        return captureMode == .meeting
            && activeCaptureState == .idle
            && !permissionsManager.canCaptureMeeting
    }

    private var meetingStatusText: String {
        switch meetingController.state {
        case .idle: "Ready for an explicit meeting capture"
        case .starting: "Connecting audio sources…"
        case .recording: "Capturing microphone and system audio"
        case .stopping: "Finalizing transcript and recording…"
        }
    }

    private var meetingProviderDisplayName: String {
        providerDisplayName(
            for: meetingController.activeProviderKind
                ?? AppSettings.shared.providerPreference
        )
    }

    private func toggleActiveCapture() {
        if captureMode == .meeting {
            appDelegate.toggleMeeting()
        } else {
            appDelegate.toggleRecording()
        }
    }

    private var statusText: String {
        switch recordingController.state {
        case .idle:     return "Ready"
        case .starting: return "Connecting — wait before speaking…"
        case .recording: return appDelegate.isHandsFree ? "Recording hands-free" : "Recording"
        case .stopping: return "Finishing..."
        }
    }

    private var buttonText: String {
        switch recordingController.state {
        case .idle:     return "Start Recording"
        case .starting: return "Connecting…"
        case .recording: return "Stop Recording"
        case .stopping: return "Stopping..."
        }
    }

    private var providerDisplayName: String {
        providerDisplayName(
            for: recordingController.activeProviderKind
                ?? selectedProviderPreference
        )
    }

    private func providerDisplayName(for kind: TranscriptionProviderKind) -> String {
        switch kind {
        case .automatic:
            return "Automatic · private-first"
        case .apple:
            return "Apple Speech · on-device"
        case .googleCloud:
            return "Google Cloud · \(AppSettings.shared.googleCloudLocation.uppercased())"
        }
    }

    private var googleAuthenticationLabel: String {
        guard !googleServiceAccountFilePath.isEmpty else {
            return "Application Default Credentials"
        }
        return URL(fileURLWithPath: googleServiceAccountFilePath).lastPathComponent
    }

    private func chooseGoogleServiceAccountFile() {
        let panel = NSOpenPanel()
        panel.allowedContentTypes = [.json]
        panel.allowsMultipleSelection = false
        panel.canChooseDirectories = false
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                _ = try GoogleServiceAccountCredential.load(from: url)
                googleServiceAccountFilePath = url.path
                AppSettings.shared.googleServiceAccountFilePath = url.path
                googleCredentialError = nil
            } catch {
                googleCredentialError = error.localizedDescription
            }
        }
    }

    private func exportDiagnostics() {
        let panel = NSSavePanel()
        panel.nameFieldStringValue = "Mluva diagnostics.json"
        panel.allowedContentTypes = [.json]
        panel.canCreateDirectories = true
        panel.begin { response in
            guard response == .OK, let url = panel.url else { return }
            do {
                let data = try TranscriptionDiagnosticsExporter().export(
                    settings: .shared,
                    entries: transcriptionStore.entries
                )
                try data.write(to: url, options: .atomic)
                diagnosticsExportError = nil
            } catch {
                diagnosticsExportError = error.localizedDescription
            }
        }
    }
}

// MARK: - Hotkey Recorder (NSViewRepresentable)

/// A key capture field that records a chord or a single physical modifier key.
struct HotkeyRecorderView: NSViewRepresentable {
    @Binding var isRecording: Bool
    var onCapture: (UInt16, CGEventFlags) -> Void

    func makeNSView(context: Context) -> HotkeyRecorderNSView {
        let view = HotkeyRecorderNSView()
        view.onCapture = onCapture
        return view
    }

    func updateNSView(_ nsView: HotkeyRecorderNSView, context: Context) {
        nsView.onCapture = onCapture
        if isRecording {
            nsView.window?.makeFirstResponder(nsView)
        } else {
            // When recording stops (e.g. Cancel pressed), resign first responder
            // so the popover isn't left without one.
            if nsView.window?.firstResponder === nsView {
                nsView.window?.makeFirstResponder(nil)
            }
        }
    }
}

final class HotkeyRecorderNSView: NSView {
    var onCapture: ((UInt16, CGEventFlags) -> Void)?

    private var modifierOnlyCandidate: (keyCode: UInt16, modifiers: CGEventFlags)?
    private let label = NSTextField(labelWithString: "Press a shortcut...")

    override init(frame frameRect: NSRect) {
        super.init(frame: frameRect)
        setup()
    }

    required init?(coder: NSCoder) {
        super.init(coder: coder)
        setup()
    }

    private func setup() {
        label.font = NSFont.systemFont(ofSize: 11)
        label.textColor = .secondaryLabelColor
        label.translatesAutoresizingMaskIntoConstraints = false
        addSubview(label)

        wantsLayer = true
        layer?.cornerRadius = 4
        layer?.borderWidth = 1
        layer?.borderColor = NSColor.systemBlue.cgColor

        NSLayoutConstraint.activate([
            label.centerYAnchor.constraint(equalTo: centerYAnchor),
            label.leadingAnchor.constraint(equalTo: leadingAnchor, constant: 6),
            label.trailingAnchor.constraint(equalTo: trailingAnchor, constant: -6),
        ])
    }

    override var acceptsFirstResponder: Bool { true }

    override func keyDown(with event: NSEvent) {
        modifierOnlyCandidate = nil

        // Require at least one modifier (prevent bare key capture)
        let modifiers = event.modifierFlags.intersection([.control, .option, .shift, .command, .function])
        guard !modifiers.isEmpty else {
            label.stringValue = "Add a modifier key..."
            return
        }

        capture(keyCode: event.keyCode, modifiers: Self.cgEventFlags(from: modifiers))
    }

    override func flagsChanged(with event: NSEvent) {
        let modifiers = event.modifierFlags.intersection([.control, .option, .shift, .command, .function])
        let cgFlags = Self.cgEventFlags(from: modifiers)

        guard let changedModifier = AppSettings.modifierFlag(for: event.keyCode) else {
            modifierOnlyCandidate = nil
            return
        }

        if cgFlags.contains(changedModifier) {
            guard cgFlags.intersection(AppSettings.supportedHotkeyModifiers) == changedModifier else {
                modifierOnlyCandidate = nil
                label.stringValue = "Press a shortcut..."
                return
            }
            modifierOnlyCandidate = (event.keyCode, changedModifier)
            label.stringValue = "Release to use \(AppSettings.keyName(for: event.keyCode))..."
            return
        }

        guard let candidate = modifierOnlyCandidate,
              candidate.keyCode == event.keyCode
        else {
            return
        }
        modifierOnlyCandidate = nil
        capture(keyCode: candidate.keyCode, modifiers: candidate.modifiers)
    }

    override func resignFirstResponder() -> Bool {
        modifierOnlyCandidate = nil
        label.stringValue = "Press a shortcut..."
        return super.resignFirstResponder()
    }

    private func capture(keyCode: UInt16, modifiers: CGEventFlags) {
        // Resign first responder before calling onCapture, which removes the
        // view from the hierarchy. Prevents a dangling first responder in the
        // NSPopover context.
        self.window?.makeFirstResponder(nil)
        onCapture?(keyCode, modifiers)
    }

    private static func cgEventFlags(from modifiers: NSEvent.ModifierFlags) -> CGEventFlags {
        var flags = CGEventFlags()
        if modifiers.contains(.control) { flags.insert(.maskControl) }
        if modifiers.contains(.option) { flags.insert(.maskAlternate) }
        if modifiers.contains(.shift) { flags.insert(.maskShift) }
        if modifiers.contains(.command) { flags.insert(.maskCommand) }
        if modifiers.contains(.function) { flags.insert(.maskSecondaryFn) }
        return flags
    }
}
