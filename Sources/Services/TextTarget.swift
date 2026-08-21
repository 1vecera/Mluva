import AppKit
import ApplicationServices
import Foundation

protocol TextTargetRestoring: AnyObject {
    var targetApplicationName: String? { get }
    var targetBundleIdentifier: String? { get }
    var selectedText: String? { get }
    var windowTitle: String? { get }
    var nearbyText: String? { get }

    @discardableResult
    func restore() -> Bool

    @discardableResult
    func replaceSelection(with text: String) -> Bool

    func confirmInsertion(of text: String) -> Bool
}

extension TextTargetRestoring {
    var targetApplicationName: String? { nil }
    var targetBundleIdentifier: String? { nil }
    var selectedText: String? { nil }
    var windowTitle: String? { nil }
    var nearbyText: String? { nil }
    var transcriptContext: TranscriptContext {
        TranscriptContext(
            applicationName: targetApplicationName,
            windowTitle: windowTitle,
            selectedText: selectedText,
            nearbyText: nearbyText
        )
    }
    func replaceSelection(with text: String) -> Bool { false }
    func confirmInsertion(of text: String) -> Bool { false }
}

final class SystemTextTarget: TextTargetRestoring, @unchecked Sendable {
    let processIdentifier: pid_t
    let bundleIdentifier: String?
    let applicationName: String?
    let selectedText: String?
    let windowTitle: String?
    let nearbyText: String?

    var targetApplicationName: String? { applicationName }
    var targetBundleIdentifier: String? { bundleIdentifier }

    private let focusedElement: AXUIElement?
    private let selectedTextRange: CFRange?

    private init(
        processIdentifier: pid_t,
        bundleIdentifier: String?,
        applicationName: String?,
        focusedElement: AXUIElement?,
        selectedTextRange: CFRange?,
        selectedText: String?,
        windowTitle: String?,
        nearbyText: String?
    ) {
        self.processIdentifier = processIdentifier
        self.bundleIdentifier = bundleIdentifier
        self.applicationName = applicationName
        self.focusedElement = focusedElement
        self.selectedTextRange = selectedTextRange
        self.selectedText = selectedText
        self.windowTitle = windowTitle
        self.nearbyText = nearbyText
    }

    static func capture(
        processIdentifier: pid_t,
        captureSelectedText: Bool = false,
        captureSelectedTextForBundleIdentifier: ((String?) -> Bool)? = nil,
        contextAllowed: (String?) -> Bool = { _ in false }
    ) -> SystemTextTarget? {
        guard processIdentifier != ProcessInfo.processInfo.processIdentifier,
              let application = NSRunningApplication(processIdentifier: processIdentifier)
        else {
            return nil
        }

        let applicationElement = AXUIElementCreateApplication(processIdentifier)
        let capturesContext = contextAllowed(application.bundleIdentifier)
        var focusedValue: CFTypeRef?
        let result = AXUIElementCopyAttributeValue(
            applicationElement,
            kAXFocusedUIElementAttribute as CFString,
            &focusedValue
        )
        let focusedElement: AXUIElement?
        if result == .success,
           let focusedValue,
           CFGetTypeID(focusedValue) == AXUIElementGetTypeID() {
            focusedElement = (focusedValue as! AXUIElement)
        } else {
            focusedElement = nil
        }

        let focusedWindow = capturesContext
            ? elementAttribute(
                kAXFocusedWindowAttribute as CFString,
                from: applicationElement
            )
            : nil
        let capturesFocusedText = captureSelectedText
            || captureSelectedTextForBundleIdentifier?(application.bundleIdentifier) == true
            || capturesContext
        let isSecureText = capturesFocusedText && focusedElement.flatMap {
            stringAttribute(kAXSubroleAttribute as CFString, from: $0)
        } == (kAXSecureTextFieldSubrole as String)

        return SystemTextTarget(
            processIdentifier: processIdentifier,
            bundleIdentifier: application.bundleIdentifier,
            applicationName: application.localizedName,
            focusedElement: focusedElement,
            selectedTextRange: focusedElement.flatMap {
                rangeAttribute(kAXSelectedTextRangeAttribute as CFString, from: $0)
            },
            selectedText: !capturesFocusedText || isSecureText ? nil : focusedElement.flatMap {
                stringAttribute(kAXSelectedTextAttribute as CFString, from: $0)
            },
            windowTitle: focusedWindow.flatMap {
                stringAttribute(kAXTitleAttribute as CFString, from: $0)
            },
            nearbyText: !capturesContext || isSecureText ? nil : focusedElement.flatMap {
                stringAttribute(kAXValueAttribute as CFString, from: $0)
            }
        )
    }

    static func isSecureTextField(processIdentifier: pid_t) -> Bool {
        let applicationElement = AXUIElementCreateApplication(processIdentifier)
        var focusedValue: CFTypeRef?
        guard AXUIElementCopyAttributeValue(
            applicationElement,
            kAXFocusedUIElementAttribute as CFString,
            &focusedValue
        ) == .success,
        let focusedValue,
        CFGetTypeID(focusedValue) == AXUIElementGetTypeID()
        else {
            return false
        }
        let focusedElement = focusedValue as! AXUIElement
        return stringAttribute(
            kAXSubroleAttribute as CFString,
            from: focusedElement
        ) == (kAXSecureTextFieldSubrole as String)
    }

    private static func elementAttribute(
        _ attribute: CFString,
        from element: AXUIElement
    ) -> AXUIElement? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success,
              let value,
              CFGetTypeID(value) == AXUIElementGetTypeID()
        else {
            return nil
        }
        return (value as! AXUIElement)
    }

    private static func stringAttribute(
        _ attribute: CFString,
        from element: AXUIElement
    ) -> String? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success,
              let text = value as? String,
              !text.isEmpty
        else {
            return nil
        }
        return text
    }

    private static func rangeAttribute(
        _ attribute: CFString,
        from element: AXUIElement
    ) -> CFRange? {
        var value: CFTypeRef?
        guard AXUIElementCopyAttributeValue(element, attribute, &value) == .success,
              let value,
              CFGetTypeID(value) == AXValueGetTypeID()
        else {
            return nil
        }
        let rangeValue = value as! AXValue
        guard
              AXValueGetType(rangeValue) == .cfRange
        else {
            return nil
        }
        var range = CFRange()
        return AXValueGetValue(rangeValue, .cfRange, &range) ? range : nil
    }

    @discardableResult
    func restore() -> Bool {
        guard let application = NSRunningApplication(processIdentifier: processIdentifier) else {
            return false
        }

        let activated = application.activate(options: [])
        if activated {
            usleep(50_000)
        }
        guard activated,
              let focusedElement,
              AXUIElementSetAttributeValue(
                  focusedElement,
                  kAXFocusedAttribute as CFString,
                  kCFBooleanTrue
              ) == .success
        else {
            return false
        }
        guard let selectedTextRange else { return true }
        return setSelectedTextRange(selectedTextRange, on: focusedElement)
    }

    @discardableResult
    func replaceSelection(with text: String) -> Bool {
        guard let focusedElement else { return false }
        if let selectedTextRange,
           !setSelectedTextRange(selectedTextRange, on: focusedElement) {
            return false
        }
        return AXUIElementSetAttributeValue(
            focusedElement,
            kAXSelectedTextAttribute as CFString,
            text as CFString
        ) == .success
    }

    func confirmInsertion(of text: String) -> Bool {
        guard let focusedElement,
              let selectedTextRange
        else {
            return false
        }
        let insertedRange = CFRange(
            location: selectedTextRange.location,
            length: text.utf16.count
        )
        let expectedCaretRange = CFRange(
            location: insertedRange.location + insertedRange.length,
            length: 0
        )
        let currentRange = Self.rangeAttribute(
            kAXSelectedTextRangeAttribute as CFString,
            from: focusedElement
        )
        if let currentRange,
           currentRange.location == expectedCaretRange.location,
           currentRange.length == expectedCaretRange.length {
            return true
        }
        guard setSelectedTextRange(insertedRange, on: focusedElement),
              Self.stringAttribute(
                  kAXSelectedTextAttribute as CFString,
                  from: focusedElement
              ) == text
        else {
            return false
        }
        _ = setSelectedTextRange(expectedCaretRange, on: focusedElement)
        return true
    }

    private func setSelectedTextRange(
        _ range: CFRange,
        on element: AXUIElement
    ) -> Bool {
        var mutableRange = range
        guard let value = AXValueCreate(.cfRange, &mutableRange) else { return false }
        return AXUIElementSetAttributeValue(
            element,
            kAXSelectedTextRangeAttribute as CFString,
            value
        ) == .success
    }
}

final class StoredApplicationTarget: TextTargetRestoring, @unchecked Sendable {
    let targetBundleIdentifier: String?
    let targetApplicationName: String?

    init(bundleIdentifier: String?, applicationName: String?) {
        targetBundleIdentifier = bundleIdentifier
        targetApplicationName = applicationName
    }

    @discardableResult
    func restore() -> Bool {
        guard let targetBundleIdentifier,
              let application = NSRunningApplication.runningApplications(
                  withBundleIdentifier: targetBundleIdentifier
              ).first
        else {
            return false
        }

        let activated = application.activate(options: [])
        if activated {
            usleep(50_000)
        }
        return activated
    }
}

final class ApplicationFocusTracker {
    static let shared = ApplicationFocusTracker()

    private let lock = NSLock()
    private var lastExternalProcessIdentifier: pid_t?
    private var observers: [NSObjectProtocol] = []

    private init() {
        rememberIfExternal(NSWorkspace.shared.frontmostApplication)

        let center = NSWorkspace.shared.notificationCenter
        let names: [Notification.Name] = [
            NSWorkspace.didActivateApplicationNotification,
            NSWorkspace.didDeactivateApplicationNotification,
        ]
        observers = names.map { name in
            center.addObserver(forName: name, object: nil, queue: .main) { [weak self] notification in
                let application = notification.userInfo?[NSWorkspace.applicationUserInfoKey]
                    as? NSRunningApplication
                self?.rememberIfExternal(application)
            }
        }
    }

    func captureTarget(
        captureSelectedText: Bool = false,
        captureSelectedTextForBundleIdentifier: ((String?) -> Bool)? = nil,
        contextAllowed: (String?) -> Bool = { _ in false }
    ) -> SystemTextTarget? {
        if let frontmost = NSWorkspace.shared.frontmostApplication,
           frontmost.processIdentifier != ProcessInfo.processInfo.processIdentifier {
            rememberIfExternal(frontmost)
            return SystemTextTarget.capture(
                processIdentifier: frontmost.processIdentifier,
                captureSelectedText: captureSelectedText,
                captureSelectedTextForBundleIdentifier: captureSelectedTextForBundleIdentifier,
                contextAllowed: contextAllowed
            )
        }

        let processIdentifier = lock.withLock { lastExternalProcessIdentifier }
        return processIdentifier.flatMap {
            SystemTextTarget.capture(
                processIdentifier: $0,
                captureSelectedText: captureSelectedText,
                captureSelectedTextForBundleIdentifier: captureSelectedTextForBundleIdentifier,
                contextAllowed: contextAllowed
            )
        }
    }

    func typedSnippetContext() -> TypedSnippetApplicationContext {
        let application: NSRunningApplication?
        if let frontmost = NSWorkspace.shared.frontmostApplication,
           frontmost.processIdentifier != ProcessInfo.processInfo.processIdentifier {
            rememberIfExternal(frontmost)
            application = frontmost
        } else {
            let processIdentifier = lock.withLock { lastExternalProcessIdentifier }
            application = processIdentifier.flatMap(NSRunningApplication.init(processIdentifier:))
        }
        guard let application else {
            return TypedSnippetApplicationContext(
                bundleIdentifier: nil,
                isSecureTextField: false
            )
        }
        return TypedSnippetApplicationContext(
            bundleIdentifier: application.bundleIdentifier,
            isSecureTextField: SystemTextTarget.isSecureTextField(
                processIdentifier: application.processIdentifier
            )
        )
    }

    private func rememberIfExternal(_ application: NSRunningApplication?) {
        guard let application,
              application.processIdentifier != ProcessInfo.processInfo.processIdentifier
        else {
            return
        }
        lock.withLock {
            lastExternalProcessIdentifier = application.processIdentifier
        }
    }

    deinit {
        let center = NSWorkspace.shared.notificationCenter
        observers.forEach(center.removeObserver)
    }
}

private extension NSLock {
    func withLock<T>(_ operation: () throws -> T) rethrows -> T {
        lock()
        defer { unlock() }
        return try operation()
    }
}
