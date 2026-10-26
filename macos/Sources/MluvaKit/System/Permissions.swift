import AppKit
import ApplicationServices
import AVFoundation
import CoreGraphics
import Observation

/// The macOS privacy permissions Mluva uses, and what each one enables.
public enum Permission: String, CaseIterable, Sendable, Identifiable {
    case microphone
    case accessibility
    case inputMonitoring
    case screenRecording

    public var id: String { rawValue }

    public var title: String {
        switch self {
        case .microphone: "Microphone"
        case .accessibility: "Accessibility"
        case .inputMonitoring: "Input Monitoring"
        case .screenRecording: "Screen Recording"
        }
    }

    public var purpose: String {
        switch self {
        case .microphone:
            "Records your voice while you dictate. Nothing is recorded until you press the shortcut."
        case .accessibility:
            "Pastes a finished dictation into the exact field you started from. Without it, Mluva copies to the clipboard."
        case .inputMonitoring:
            "Lets screen annotation notice when you hold Shift to draw while you talk."
        case .screenRecording:
            "Lets screen annotation capture what you marked, so an agent can see it."
        }
    }

    public var symbol: String {
        switch self {
        case .microphone: "mic.fill"
        case .accessibility: "accessibility"
        case .inputMonitoring: "keyboard"
        case .screenRecording: "rectangle.dashed.badge.record"
        }
    }

    /// Required for dictation itself; the others enable optional features.
    public var isRequired: Bool { self == .microphone }

    var settingsAnchor: String {
        switch self {
        case .microphone: "Privacy_Microphone"
        case .accessibility: "Privacy_Accessibility"
        case .inputMonitoring: "Privacy_ListenEvent"
        case .screenRecording: "Privacy_ScreenCapture"
        }
    }
}

public enum PermissionState: Sendable, Equatable {
    case granted
    case notAsked
    case denied
}

@MainActor
@Observable
public final class PermissionCenter {
    public private(set) var states: [Permission: PermissionState] = [:]

    public init() {
        refresh()
    }

    public func state(_ permission: Permission) -> PermissionState {
        states[permission] ?? .notAsked
    }

    public func refresh() {
        var next: [Permission: PermissionState] = [:]
        switch AVCaptureDevice.authorizationStatus(for: .audio) {
        case .authorized: next[.microphone] = .granted
        case .notDetermined: next[.microphone] = .notAsked
        default: next[.microphone] = .denied
        }
        next[.accessibility] = AXIsProcessTrusted() ? .granted : .notAsked
        next[.inputMonitoring] = CGPreflightListenEventAccess() ? .granted : .notAsked
        next[.screenRecording] = CGPreflightScreenCaptureAccess() ? .granted : .notAsked
        if next != states { states = next }
    }

    /// Ask macOS. Microphone shows the system prompt; the others open their Settings pane,
    /// where the person switches Mluva on.
    public func request(_ permission: Permission) {
        switch permission {
        case .microphone:
            if AVCaptureDevice.authorizationStatus(for: .audio) == .notDetermined {
                AVCaptureDevice.requestAccess(for: .audio) { _ in
                    Task { @MainActor in self.refresh() }
                }
                return
            }
        case .accessibility:
            let prompt = "AXTrustedCheckOptionPrompt" as CFString
            _ = AXIsProcessTrustedWithOptions([prompt: true] as CFDictionary)
        case .inputMonitoring:
            _ = CGRequestListenEventAccess()
        case .screenRecording:
            _ = CGRequestScreenCaptureAccess()
        }
        openSettings(permission)
    }

    public func openSettings(_ permission: Permission) {
        let url = "x-apple.systempreferences:com.apple.preference.security?\(permission.settingsAnchor)"
        if let url = URL(string: url) { NSWorkspace.shared.open(url) }
    }
}
