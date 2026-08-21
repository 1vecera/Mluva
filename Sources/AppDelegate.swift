import AppKit
import SwiftUI
import Combine

@MainActor
final class AppDelegate: NSObject, NSApplicationDelegate, ObservableObject {
    private var statusItem: NSStatusItem!
    private var popover: NSPopover!

    let recordingController = RecordingController()
    let permissionsManager = PermissionsManager()
    let hotkeyManager = GlobalHotkeyManager()
    let transcriptionStore = TranscriptionStore()
    let meetingStore = MeetingStore()
    lazy var meetingController = MeetingController(meetingStore: meetingStore)
    let personalizationStore = PersonalizationStore.shared
    let transcriptionRecoveryService = TranscriptionRecoveryService()

    @Published var isRecording = false
    @Published var isHandsFree = false
    @Published var busyHistoryEntryIDs: Set<UUID> = []
    @Published var historyActionError: String?

    private var cancellables = Set<AnyCancellable>()

    // MARK: - Lifecycle

    func applicationDidFinishLaunching(_ notification: Notification) {
        NSApp.setActivationPolicy(.accessory) // menu-bar-only, no dock icon

        setupStatusItem()
        loadHotkeyConfig()
        setupHotkey()
        setupAccessibilityCheck()
        observeRecordingState()
        observeMeetingState()

        if !AppSettings.shared.hasCompletedSetup
            || permissionsManager.missingRecognitionRequirement() != nil
            || recordingController.pendingScratchpadDraft != nil {
            DispatchQueue.main.asyncAfter(deadline: .now() + 0.5) { [weak self] in
                self?.showPopover()
            }
        }
    }

    // MARK: - Status Bar

    private func setupStatusItem() {
        statusItem = NSStatusBar.system.statusItem(withLength: NSStatusItem.variableLength)

        if let button = statusItem.button {
            button.image = NSImage(systemSymbolName: "mic", accessibilityDescription: "Mluva")
            button.action = #selector(statusBarButtonClicked(_:))
            button.sendAction(on: [.leftMouseUp, .rightMouseUp])
        }

        let popover = NSPopover()
        popover.contentSize = NSSize(width: 350, height: 440)
        popover.behavior = .transient
        popover.contentViewController = NSHostingController(
            rootView: StatusBarPopover()
                .environmentObject(self)
                .environmentObject(recordingController)
                .environmentObject(permissionsManager)
                .environmentObject(transcriptionStore)
                .environmentObject(meetingController)
                .environmentObject(meetingStore)
                .environmentObject(personalizationStore)
        )
        self.popover = popover
    }

    @objc private func statusBarButtonClicked(_ sender: NSStatusBarButton) {
        let event = NSApp.currentEvent!

        if event.type == .rightMouseUp {
            // Right-click: show context menu
            let hotkeyLabel = hotkeyManager.displayString
            let menu = NSMenu()
            menu.addItem(NSMenuItem(title: "Toggle Recording (\(hotkeyLabel))", action: #selector(toggleRecordingAction), keyEquivalent: ""))
            menu.addItem(.separator())
            menu.addItem(NSMenuItem(title: "Quit Mluva", action: #selector(quitAction), keyEquivalent: "q"))
            statusItem.menu = menu
            statusItem.button?.performClick(nil)
            statusItem.menu = nil // reset so left-click shows popover again
        } else {
            togglePopover()
        }
    }

    @objc private func toggleRecordingAction() { toggleRecording() }
    @objc private func quitAction() { NSApplication.shared.terminate(nil) }

    // MARK: - Popover

    private func togglePopover() {
        if popover.isShown {
            popover.performClose(nil)
        } else {
            showPopover()
        }
    }

    func showPopover() {
        guard let button = statusItem.button else { return }
        popover.show(relativeTo: button.bounds, of: button, preferredEdge: .minY)
        NSApp.activate(ignoringOtherApps: true)
    }

    // MARK: - Global Hotkey

    private func loadHotkeyConfig() {
        let settings = AppSettings.shared
        hotkeyManager.configure(
            keyCode: settings.hotkeyKeyCode,
            modifiers: settings.hotkeyModifierFlags
        )
    }

    private func setupHotkey() {
        hotkeyManager.shouldExpandTypedSnippets = { [weak self] in
            AppSettings.shared.typedSnippetExpansionEnabled
                && self?.recordingController.state == .idle
        }
        hotkeyManager.onStartCapture = { [weak self] in
            self?.startRecording()
        }
        hotkeyManager.onStopCapture = { [weak self] in
            self?.stopRecording()
        }
        hotkeyManager.onCancelCapture = { [weak self] in
            self?.cancelRecording()
        }
        hotkeyManager.onHandsFreeChanged = { [weak self] enabled in
            self?.isHandsFree = enabled
        }

        // Only start hotkey manager if accessibility is already granted
        if permissionsManager.accessibilityGranted {
            hotkeyManager.start()
        }

        // Watch for accessibility to transition from false -> true
        permissionsManager.$accessibilityGranted
            .removeDuplicates()
            .sink { [weak self] granted in
                if granted {
                    self?.hotkeyManager.start()
                }
            }
            .store(in: &cancellables)
    }

    /// Called when the user changes the hotkey in settings
    func updateHotkey(keyCode: UInt16, modifiers: CGEventFlags) {
        let settings = AppSettings.shared
        settings.hotkeyKeyCode = keyCode
        settings.hotkeyModifierFlags = modifiers

        hotkeyManager.configure(keyCode: keyCode, modifiers: modifiers)
    }

    // MARK: - Accessibility Check for RecordingController

    private func setupAccessibilityCheck() {
        recordingController.accessibilityCheck = { [weak self] in
            self?.permissionsManager.accessibilityGranted ?? false
        }
    }

    // MARK: - Recording

    func toggleRecording() {
        if isRecording {
            stopRecording()
        } else {
            startRecording()
        }
    }

    func startRecording() {
        guard !isRecording else { return }
        guard meetingController.state == .idle else {
            showPopover()
            return
        }
        guard AppSettings.shared.transcriptionMode != .meeting else {
            showPopover()
            return
        }
        guard permissionsManager.missingRecognitionRequirement() == nil else {
            showPopover()
            return
        }

        isRecording = true
        updateStatusIcon(recording: true)

        recordingController.start { [weak self] in
            DispatchQueue.main.async {
                self?.isRecording = false
                self?.updateStatusIcon(recording: false)
            }
        }
    }

    func stopRecording() {
        guard isRecording else { return }
        isRecording = false
        updateStatusIcon(recording: false)

        recordingController.stop { [weak self] _ in
            if self?.recordingController.lastSessionWasIncognito == false,
               self?.recordingController.pendingCommandPreview == nil,
               self?.recordingController.pendingScratchpadDraft == nil,
               let result = self?.recordingController.lastSessionResult {
                self?.transcriptionStore.save(entry: result)
            }
            if self?.recordingController.pendingCommandPreview != nil
                || self?.recordingController.pendingScratchpadDraft != nil
                || self?.recordingController.lastSessionResult?.deliveryOutcome
                    == .pendingDelivery {
                self?.showPopover()
            }
        }
    }

    func cancelRecording() {
        guard isRecording else { return }
        isRecording = false
        isHandsFree = false
        updateStatusIcon(recording: false)
        recordingController.cancel()
    }

    // MARK: - Meeting Capture

    func toggleMeeting() {
        switch meetingController.state {
        case .idle:
            startMeeting()
        case .recording:
            stopMeeting()
        case .starting, .stopping:
            break
        }
    }

    func startMeeting() {
        guard recordingController.state == .idle,
              meetingController.state == .idle
        else {
            return
        }
        guard permissionsManager.missingRecognitionRequirement() == nil,
              permissionsManager.canCaptureMeeting
        else {
            showPopover()
            return
        }

        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                try await meetingController.start()
            } catch {
                showPopover()
            }
        }
    }

    func stopMeeting() {
        guard meetingController.state == .recording else { return }
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                _ = try await meetingController.stop()
            } catch {
                showPopover()
            }
        }
    }

    func cancelMeeting() {
        guard meetingController.state != .idle else { return }
        Task { @MainActor [weak self] in
            await self?.meetingController.cancel()
        }
    }

    func acceptCommandPreview() {
        historyActionError = nil
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                if let entry = try await recordingController.acceptCommandPreview(),
                   !recordingController.lastSessionWasIncognito {
                    transcriptionStore.save(entry: entry)
                }
            } catch {
                historyActionError = error.localizedDescription
            }
        }
    }

    func discardCommandPreview() {
        recordingController.discardCommandPreview()
        historyActionError = nil
    }

    func copyScratchpadDraft() {
        acceptScratchpadDraft(destination: .clipboard)
    }

    func insertScratchpadDraft() {
        acceptScratchpadDraft(destination: .originalApplication)
    }

    func applyScratchpadStyle() {
        historyActionError = nil
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                _ = try await recordingController.applyStyleToScratchpadDraft()
            } catch {
                historyActionError = error.localizedDescription
            }
        }
    }

    func discardScratchpadDraft() {
        recordingController.discardScratchpadDraft()
        historyActionError = nil
    }

    private func acceptScratchpadDraft(
        destination: ScratchpadDraftDestination
    ) {
        historyActionError = nil
        Task { @MainActor [weak self] in
            guard let self else { return }
            do {
                if let entry = try await recordingController.acceptScratchpadDraft(
                    destination: destination
                ), !recordingController.lastSessionWasIncognito {
                    transcriptionStore.save(entry: entry)
                }
            } catch {
                historyActionError = error.localizedDescription
            }
        }
    }

    func retryRecognition(_ entry: TranscriptionEntry) {
        guard busyHistoryEntryIDs.insert(entry.id).inserted else { return }
        historyActionError = nil

        Task { @MainActor [weak self] in
            guard let self else { return }
            defer { busyHistoryEntryIDs.remove(entry.id) }
            do {
                let recovered = try await transcriptionRecoveryService.retryRecognition(
                    entry: entry,
                    settings: .shared
                )
                transcriptionStore.update(entry: recovered)
            } catch {
                let message = error.localizedDescription
                transcriptionStore.update(
                    entry: entry.updatingDelivery(
                        outcome: .failed,
                        failureMessage: message
                    )
                )
                historyActionError = message
            }
        }
    }

    func retryDelivery(_ entry: TranscriptionEntry) {
        guard entry.canDeliverFromHistory,
              busyHistoryEntryIDs.insert(entry.id).inserted
        else {
            return
        }
        historyActionError = nil

        Task { @MainActor [weak self] in
            guard let self else { return }
            defer { busyHistoryEntryIDs.remove(entry.id) }

            let hasStoredTarget = entry.targetBundleIdentifier != nil
                || entry.targetApplicationName != nil
            let canPasteIntoTarget = permissionsManager.accessibilityGranted && hasStoredTarget
            let target: (any TextTargetRestoring)? = hasStoredTarget
                ? StoredApplicationTarget(
                    bundleIdentifier: entry.targetBundleIdentifier,
                    applicationName: entry.targetApplicationName
                )
                : nil
            let destination = KeyboardTextDestination(
                keyboardSimulator: recordingController.keyboardSimulator,
                canPaste: { canPasteIntoTarget },
                target: target
            )
            let coordinator = DeliveryCoordinator(destination: destination)

            do {
                _ = try await coordinator.deliver(
                    .final(id: "history-\(entry.id.uuidString)", text: entry.deliveredText)
                )
                if canPasteIntoTarget {
                    transcriptionStore.update(
                        entry: entry.updatingDelivery(outcome: .delivered)
                    )
                } else if entry.deliveryOutcome == .pendingDelivery {
                    transcriptionStore.update(entry: entry.updatingDelivery(
                        outcome: .pendingDelivery,
                        failureMessage: "Copied to the clipboard. Paste it where you want."
                    ))
                }
            } catch {
                let message = error.localizedDescription
                if entry.deliveryOutcome == .pendingDelivery {
                    transcriptionStore.update(entry: entry.updatingDelivery(
                        outcome: .pendingDelivery,
                        failureMessage: message
                    ))
                }
                historyActionError = message
            }
        }
    }

    func reprocess(_ entry: TranscriptionEntry) {
        guard !entry.rawText.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty else {
            historyActionError = "This history entry has no raw transcript to reprocess."
            return
        }

        let processed = TranscriptProcessor().process(
            entry.rawText,
            configuration: personalizationStore.processingConfiguration(
                removeFillers: AppSettings.shared.removeFiller,
                targetBundleIdentifier: entry.targetBundleIdentifier
            )
        )
        transcriptionStore.update(
            entry: entry.reprocessed(deliveredText: processed.text)
        )
        historyActionError = nil
    }

    // MARK: - Status Icon

    private func updateStatusIcon(recording: Bool) {
        guard let button = statusItem.button else { return }
        if recording {
            button.image = NSImage(systemSymbolName: "mic.fill", accessibilityDescription: "Recording")
            button.contentTintColor = .systemRed
        } else {
            button.image = NSImage(systemSymbolName: "mic", accessibilityDescription: "Mluva")
            button.contentTintColor = nil
        }
    }

    // MARK: - Observe Recording State

    private func observeRecordingState() {
        recordingController.$state
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                let recording = state == .recording || state == .starting
                self?.isRecording = recording
                self?.updateStatusIcon(recording: recording)
            }
            .store(in: &cancellables)
    }

    private func observeMeetingState() {
        meetingController.$state
            .receive(on: DispatchQueue.main)
            .sink { [weak self] state in
                let recording = state == .recording || state == .starting
                self?.updateStatusIcon(recording: recording)
            }
            .store(in: &cancellables)
    }
}
