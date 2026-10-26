import AppKit
import ApplicationServices
import Carbon.HIToolbox

/// The application and field focused when a global recording started.
public struct CapturedTarget: @unchecked Sendable {
    public let pid: pid_t
    public let bundleIdentifier: String?
    public let applicationName: String
    /// Accessibility elements, present only with Accessibility permission.
    let window: AXUIElement?
    let element: AXUIElement?
    /// A password field or secure input: never typed into.
    public let isSecure: Bool

    public var canInsert: Bool { element != nil && !isSecure }
}

/// What happened to a finished dictation, in the shared History vocabulary.
public struct DeliveryReceipt: Sendable, Equatable {
    public enum Outcome: String, Sendable {
        case pasted
        case pasteUnconfirmed = "paste-unconfirmed"
        case copied
        case ready
    }

    public var outcome: Outcome
    public var guidance: String
}

/// Clipboard-first delivery with at most one paste into the exact captured target.
@MainActor
public enum TextDelivery {
    /// Remember the focused application and field; nil when Mluva itself is frontmost.
    public static func captureTarget() -> CapturedTarget? {
        guard let app = NSWorkspace.shared.frontmostApplication,
              app.processIdentifier != ProcessInfo.processInfo.processIdentifier else { return nil }
        var window: AXUIElement?
        var element: AXUIElement?
        var secure = IsSecureEventInputEnabled()
        if AXIsProcessTrusted() {
            let application = AXUIElementCreateApplication(app.processIdentifier)
            window = attribute(application, kAXFocusedWindowAttribute)
            element = attribute(application, kAXFocusedUIElementAttribute)
            if let element, let subrole: String = attribute(element, kAXSubroleAttribute) {
                secure = secure || subrole == (kAXSecureTextFieldSubrole as String)
            }
        }
        return CapturedTarget(
            pid: app.processIdentifier, bundleIdentifier: app.bundleIdentifier,
            applicationName: app.localizedName ?? "the app", window: window, element: element,
            isSecure: secure)
    }

    /// Copy `text`; paste it only when insertion is allowed and the exact target is still
    /// focused. A paste is never retried, and a changed target stays clipboard-only.
    public static func deliver(_ text: String, to target: CapturedTarget?, allowInsertion: Bool) async -> DeliveryReceipt {
        guard !text.isEmpty else { return DeliveryReceipt(outcome: .ready, guidance: "Nothing to deliver.") }
        copy(text)
        guard allowInsertion, let target else {
            return DeliveryReceipt(outcome: .copied, guidance: "Copied. Paste it where you need it.")
        }
        guard AXIsProcessTrusted() else {
            return DeliveryReceipt(outcome: .copied, guidance: "Copied. Allow Accessibility to paste automatically.")
        }
        guard target.canInsert, stillFocused(target), !IsSecureEventInputEnabled() else {
            return DeliveryReceipt(outcome: .copied, guidance: "Copied. The original field changed, so Mluva did not paste.")
        }
        let before = caret(of: target.element)
        postPaste()
        let expected = before.map { $0.location + text.utf16.count }
        for _ in 0..<6 {
            try? await Task.sleep(for: .milliseconds(100))
            if let expected, let after = caret(of: target.element), after.location == expected {
                return DeliveryReceipt(outcome: .pasted, guidance: "Pasted into \(target.applicationName).")
            }
        }
        return DeliveryReceipt(
            outcome: .pasteUnconfirmed,
            guidance: "Pasted into \(target.applicationName); Mluva could not confirm it. The text is also on the clipboard.")
    }

    public static func copy(_ text: String) {
        let board = NSPasteboard.general
        board.clearContents()
        board.setString(text, forType: .string)
    }

    private static func stillFocused(_ target: CapturedTarget) -> Bool {
        guard NSWorkspace.shared.frontmostApplication?.processIdentifier == target.pid else { return false }
        let application = AXUIElementCreateApplication(target.pid)
        let window: AXUIElement? = attribute(application, kAXFocusedWindowAttribute)
        let element: AXUIElement? = attribute(application, kAXFocusedUIElementAttribute)
        if let captured = target.window, let window, !CFEqual(captured, window) { return false }
        guard let captured = target.element, let element else { return false }
        return CFEqual(captured, element)
    }

    /// The caret's UTF-16 range; never the field's text.
    private static func caret(of element: AXUIElement?) -> CFRange? {
        guard let element, let value: AXValue = attribute(element, kAXSelectedTextRangeAttribute) else { return nil }
        var range = CFRange()
        return AXValueGetValue(value, .cfRange, &range) ? range : nil
    }

    private static func postPaste() {
        let source = CGEventSource(stateID: .combinedSessionState)
        let down = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: true)
        let up = CGEvent(keyboardEventSource: source, virtualKey: CGKeyCode(kVK_ANSI_V), keyDown: false)
        down?.flags = .maskCommand
        up?.flags = .maskCommand
        down?.post(tap: .cghidEventTap)
        up?.post(tap: .cghidEventTap)
    }

    private static func attribute<T>(_ element: AXUIElement, _ name: String) -> T? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, name as CFString, &value) == .success else { return nil }
        return value as? T
    }
}
