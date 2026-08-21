import AVFoundation
import ApplicationServices
import CoreGraphics
import Foundation
import AppKit
import Speech

final class PermissionsManager: ObservableObject {
    @Published var microphoneGranted = false
    @Published var speechRecognitionGranted = false
    @Published var accessibilityGranted = false
    @Published var screenRecordingGranted = false

    private var pollTimer: Timer?

    init() {
        checkAll()
        // Re-check when app becomes active (user returning from System Settings)
        NotificationCenter.default.addObserver(
            self,
            selector: #selector(appDidBecomeActive),
            name: NSApplication.didBecomeActiveNotification,
            object: nil
        )
    }

    /// Microphone is sufficient for recording; accessibility enhances the experience
    var canRecord: Bool { microphoneGranted }
    var canUseAppleSpeech: Bool { microphoneGranted && speechRecognitionGranted }
    var canCaptureMeeting: Bool { microphoneGranted && screenRecordingGranted }

    func missingRecognitionRequirement(
        settings: AppSettings = .shared
    ) -> RecognitionPermissionRequirement? {
        RecognitionPermissionPolicy.missingRequirement(
            preference: settings.providerPreference,
            microphoneGranted: microphoneGranted,
            appleSpeechGranted: speechRecognitionGranted,
            cloudFallbackAvailable: settings.cloudFallbackAvailable
        )
    }

    @objc private func appDidBecomeActive() {
        checkAll()
        if allGranted {
            stopPolling()
        }
    }

    func checkAll() {
        checkMicrophone()
        checkSpeechRecognition()
        checkAccessibility()
        checkScreenRecording()
    }

    // MARK: - Microphone

    func checkMicrophone() {
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized:
            microphoneGranted = true
        default:
            microphoneGranted = false
        }
    }

    func requestMicrophone() {
        AVCaptureDevice.requestAccess(for: .audio) { [weak self] granted in
            DispatchQueue.main.async {
                self?.microphoneGranted = granted
            }
        }
    }

    // MARK: - Speech Recognition

    func checkSpeechRecognition() {
        speechRecognitionGranted = SFSpeechRecognizer.authorizationStatus() == .authorized
    }

    func requestSpeechRecognition() {
        SFSpeechRecognizer.requestAuthorization { [weak self] status in
            DispatchQueue.main.async {
                self?.speechRecognitionGranted = status == .authorized
            }
        }
    }

    // MARK: - Accessibility (needed for global hotkey + keyboard simulation)

    func checkAccessibility() {
        accessibilityGranted = AXIsProcessTrusted()
    }

    func requestAccessibility() {
        let options = [kAXTrustedCheckOptionPrompt.takeUnretainedValue(): true] as CFDictionary
        _ = AXIsProcessTrustedWithOptions(options)
        // Start polling — user must toggle in System Settings, we can't detect the moment
        startPolling()
    }

    // MARK: - Screen Recording (needed only for explicit meeting system audio)

    func checkScreenRecording() {
        screenRecordingGranted = CGPreflightScreenCaptureAccess()
    }

    func requestScreenRecording() {
        screenRecordingGranted = CGRequestScreenCaptureAccess()
        if !screenRecordingGranted {
            openScreenRecordingSettings()
        }
    }

    func openScreenRecordingSettings() {
        NSWorkspace.shared.open(
            URL(
                string: "x-apple.systempreferences:com.apple.preference.security?Privacy_ScreenCapture"
            )!
        )
    }

    // MARK: - Polling (for permission changes that happen in System Settings)

    func startPolling() {
        guard pollTimer == nil else { return }
        pollTimer = Timer.scheduledTimer(withTimeInterval: 1.5, repeats: true) { [weak self] _ in
            self?.checkAll()
            if self?.allGranted == true {
                self?.stopPolling()
            }
        }
    }

    func stopPolling() {
        pollTimer?.invalidate()
        pollTimer = nil
    }

    var isPolling: Bool { pollTimer != nil }

    var allGranted: Bool {
        microphoneGranted && speechRecognitionGranted && accessibilityGranted
    }

    func openAccessibilitySettings() {
        NSWorkspace.shared.open(
            URL(string: "x-apple.systempreferences:com.apple.preference.security?Privacy_Accessibility")!
        )
    }

    deinit {
        stopPolling()
        NotificationCenter.default.removeObserver(self)
    }
}
