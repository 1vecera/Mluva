import CoreGraphics
import Foundation

final class GlobalHotkeyManager {
    var onToggle: (() -> Void)?
    var onStartCapture: (() -> Void)?
    var onStopCapture: (() -> Void)?
    var onCancelCapture: (() -> Void)?
    var onHandsFreeChanged: ((Bool) -> Void)?
    var shouldExpandTypedSnippets: () -> Bool = {
        AppSettings.shared.typedSnippetExpansionEnabled
    }

    private var port: CFMachPort?
    private var runLoopSource: CFRunLoopSource?
    private var retainedSelf: UnsafeMutableRawPointer?
    private var gestureInterpreter = HotkeyGestureInterpreter()
    private var isHandlingHotkeyKeyPress = false
    private var modifierOnlyCaptureBegan = false
    private var pendingBeginHold: DispatchWorkItem?
    private var pendingEndHold: DispatchWorkItem?
    private let modifierOnlyActivationDelay: TimeInterval
    private let personalizationStore: PersonalizationStore
    private let typedSnippetReplacer: any TypedSnippetReplacing
    private let snippetContext: () -> TypedSnippetApplicationContext
    private let snippetVariables: () -> [String: String]
    private var typedSnippetMatcher = TypedSnippetMatcher()
    private var typedSnippetBundleIdentifier: String?

    // Configurable hotkey — default: Right Command
    private(set) var hotkeyKeyCode: UInt16 = UInt16(AppSettings.defaultHotkeyKeyCode)
    private(set) var hotkeyModifiers: CGEventFlags = CGEventFlags(rawValue: AppSettings.defaultHotkeyModifiers)

    init(
        personalizationStore: PersonalizationStore = .shared,
        typedSnippetReplacer: any TypedSnippetReplacing = KeyboardSimulator(),
        snippetContext: @escaping () -> TypedSnippetApplicationContext = {
            ApplicationFocusTracker.shared.typedSnippetContext()
        },
        snippetVariables: @escaping () -> [String: String] = {
            SnippetVariableResolver().values()
        },
        modifierOnlyActivationDelay: TimeInterval = 0.15
    ) {
        self.personalizationStore = personalizationStore
        self.typedSnippetReplacer = typedSnippetReplacer
        self.snippetContext = snippetContext
        self.snippetVariables = snippetVariables
        self.modifierOnlyActivationDelay = modifierOnlyActivationDelay
    }

    /// Reconfigure the hotkey combination. If the event tap is running, it restarts
    /// to pick up the new configuration.
    func configure(keyCode: UInt16, modifiers: CGEventFlags) {
        hotkeyKeyCode = keyCode
        hotkeyModifiers = modifiers

        // Restart the event tap if already running so it uses the new combination.
        if port != nil {
            stop()
            start()
        }
    }

    func start() {
        guard port == nil else { return }

        let mask: CGEventMask = (1 << CGEventType.keyDown.rawValue)
            | (1 << CGEventType.keyUp.rawValue)
            | (1 << CGEventType.flagsChanged.rawValue)
            | (1 << CGEventType.leftMouseDown.rawValue)
            | (1 << CGEventType.rightMouseDown.rawValue)
            | (1 << CGEventType.otherMouseDown.rawValue)
        let info = Unmanaged.passRetained(self).toOpaque()

        guard let eventPort = CGEvent.tapCreate(
            tap: .cgSessionEventTap,
            place: .headInsertEventTap,
            options: .defaultTap,
            eventsOfInterest: mask,
            callback: globalHotkeyCallback,
            userInfo: info
        ) else {
            Unmanaged<GlobalHotkeyManager>.fromOpaque(info).release()
            return
        }

        self.retainedSelf = info
        self.port = eventPort
        let source = CFMachPortCreateRunLoopSource(nil, eventPort, 0)!
        self.runLoopSource = source
        CFRunLoopAddSource(CFRunLoopGetMain(), source, .commonModes)
    }

    func stop() {
        pendingBeginHold?.cancel()
        pendingBeginHold = nil
        pendingEndHold?.cancel()
        pendingEndHold = nil
        if let source = runLoopSource {
            CFRunLoopRemoveSource(CFRunLoopGetMain(), source, .commonModes)
        }
        if let port = port {
            CGEvent.tapEnable(tap: port, enable: false)
            CFMachPortInvalidate(port)
        }
        // Release the retained self that was passed to tapCreate
        if let ptr = retainedSelf {
            Unmanaged<GlobalHotkeyManager>.fromOpaque(ptr).release()
            retainedSelf = nil
        }
        port = nil
        runLoopSource = nil
        isHandlingHotkeyKeyPress = false
        modifierOnlyCaptureBegan = false
        _ = gestureInterpreter.cancel()
    }

    func isHotkey(_ event: CGEvent) -> Bool {
        let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        let flags = event.flags

        guard keyCode == hotkeyKeyCode else { return false }
        guard flags.contains(hotkeyModifiers) else { return false }

        // Reject if any extra modifiers are pressed that aren't part of the configured combo
        let allModifiers: [CGEventFlags] = [
            .maskCommand,
            .maskControl,
            .maskAlternate,
            .maskShift,
            .maskSecondaryFn,
        ]

        for modifier in allModifiers {
            if flags.contains(modifier) && !hotkeyModifiers.contains(modifier) {
                return false
            }
        }

        return true
    }

    func isConfiguredKey(_ event: CGEvent) -> Bool {
        UInt16(event.getIntegerValueField(.keyboardEventKeycode)) == hotkeyKeyCode
    }

    @discardableResult
    func handleGestureEvent(type: CGEventType, event: CGEvent) -> Bool {
        let timestamp = TimeInterval(event.timestamp) / 1_000_000_000
        let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        let usesModifierOnlyHotkey = AppSettings.isModifierOnlyHotkey(
            keyCode: hotkeyKeyCode,
            modifiers: hotkeyModifiers
        )

        if usesModifierOnlyHotkey,
           isHandlingHotkeyKeyPress,
           ((type == .keyDown && keyCode != hotkeyKeyCode)
            || (type == .flagsChanged && keyCode != hotkeyKeyCode)) {
            interruptModifierOnlyGesture()
            return false
        }

        let action: HotkeyGestureAction?
        let shouldConsume: Bool

        switch type {
        case .flagsChanged where usesModifierOnlyHotkey && isHotkey(event):
            guard !isHandlingHotkeyKeyPress else { return false }
            isHandlingHotkeyKeyPress = true
            action = gestureInterpreter.keyDown(at: timestamp, isRepeat: false)
            shouldConsume = false

        case .flagsChanged where usesModifierOnlyHotkey
            && isConfiguredKey(event)
            && isHandlingHotkeyKeyPress
            && !event.flags.contains(hotkeyModifiers):
            isHandlingHotkeyKeyPress = false
            action = gestureInterpreter.keyUp(at: timestamp)
            shouldConsume = false

        case .keyDown where !usesModifierOnlyHotkey && isHotkey(event):
            isHandlingHotkeyKeyPress = true
            let isRepeat = event.getIntegerValueField(.keyboardEventAutorepeat) != 0
            action = gestureInterpreter.keyDown(at: timestamp, isRepeat: isRepeat)
            shouldConsume = true

        case .keyUp where !usesModifierOnlyHotkey
            && isConfiguredKey(event)
            && isHandlingHotkeyKeyPress:
            isHandlingHotkeyKeyPress = false
            action = gestureInterpreter.keyUp(at: timestamp)
            shouldConsume = true

        default:
            return false
        }

        if let action {
            resetTypedSnippetBuffer()
            handle(action, forModifierOnlyHotkey: usesModifierOnlyHotkey)
        }
        return shouldConsume
    }

    @discardableResult
    func cancelModifierOnlyGesture() -> Bool {
        guard isHandlingHotkeyKeyPress,
              AppSettings.isModifierOnlyHotkey(
                keyCode: hotkeyKeyCode,
                modifiers: hotkeyModifiers
              )
        else {
            return false
        }
        return interruptModifierOnlyGesture()
    }

    @discardableResult
    func cancelGesture() -> Bool {
        guard let action = gestureInterpreter.cancel() else { return false }
        perform(action)
        return true
    }

    @discardableResult
    func handleTypedSnippetInput(
        _ text: String,
        keyCode: UInt16,
        flags: CGEventFlags
    ) -> Bool {
        guard let context = eligibleTypedSnippetContext() else { return false }
        return handleTypedSnippetInput(
            text,
            keyCode: keyCode,
            flags: flags,
            context: context
        )
    }

    @discardableResult
    private func handleTypedSnippetInput(
        _ text: String,
        keyCode: UInt16,
        flags: CGEventFlags,
        context: TypedSnippetApplicationContext
    ) -> Bool {
        let disallowedModifiers: CGEventFlags = [
            .maskCommand,
            .maskControl,
            .maskAlternate,
        ]
        guard flags.intersection(disallowedModifiers).isEmpty else {
            typedSnippetMatcher.reset()
            return false
        }

        switch keyCode {
        case 51:
            typedSnippetMatcher.deleteBackward()
            return false
        case 53, 115, 116, 117, 119, 121, 123...126:
            typedSnippetMatcher.reset()
            return false
        default:
            break
        }

        let snippets = personalizationStore.snippets(for: context.bundleIdentifier)
        let maximumLength = snippets.compactMap(\.typedTrigger).map(\.count).max() ?? 0
        guard maximumLength > 0 else {
            typedSnippetMatcher.reset()
            return false
        }
        let trailingText: String?
        if text == " " || keyCode == 49 {
            trailingText = " "
        } else if text == "\t" || keyCode == 48 {
            trailingText = "\t"
        } else if text == "\n" || text == "\r" || keyCode == 36 {
            trailingText = "\n"
        } else {
            trailingText = nil
        }

        if let trailingText {
            guard let replacement = typedSnippetMatcher.complete(
                trailingText: trailingText,
                snippets: snippets,
                variables: snippetVariables()
            ) else {
                return false
            }
            typedSnippetReplacer.replaceTypedSnippet(replacement)
            return true
        }

        guard !text.isEmpty else {
            typedSnippetMatcher.reset()
            return false
        }
        typedSnippetMatcher.append(text, maximumLength: maximumLength)
        return false
    }

    func resetTypedSnippetBuffer() {
        typedSnippetMatcher.reset()
    }

    private func eligibleTypedSnippetContext() -> TypedSnippetApplicationContext? {
        guard shouldExpandTypedSnippets() else {
            typedSnippetMatcher.reset()
            return nil
        }
        let context = snippetContext()
        guard !context.isSecureTextField else {
            typedSnippetMatcher.reset()
            return nil
        }
        if typedSnippetBundleIdentifier != context.bundleIdentifier {
            typedSnippetMatcher.reset()
            typedSnippetBundleIdentifier = context.bundleIdentifier
        }
        return context
    }

    @discardableResult
    func handleTypedSnippetEvent(_ event: CGEvent) -> Bool {
        // Check privacy before decoding any character data from the event.
        guard let context = eligibleTypedSnippetContext() else { return false }
        let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        var characters = [UniChar](repeating: 0, count: 8)
        var length = 0
        characters.withUnsafeMutableBufferPointer { buffer in
            event.keyboardGetUnicodeString(
                maxStringLength: buffer.count,
                actualStringLength: &length,
                unicodeString: buffer.baseAddress!
            )
        }
        let text = String(utf16CodeUnits: characters, count: length)
        return handleTypedSnippetInput(
            text,
            keyCode: keyCode,
            flags: event.flags,
            context: context
        )
    }

    private func perform(_ action: HotkeyGestureAction) {
        switch action {
        case .beginHold:
            pendingEndHold?.cancel()
            pendingEndHold = nil
            onStartCapture?()

        case .scheduleEndHold:
            let workItem = DispatchWorkItem { [weak self] in
                guard let self,
                      let action = self.gestureInterpreter.tapWindowExpired()
                else {
                    return
                }
                self.pendingEndHold = nil
                self.perform(action)
            }
            pendingEndHold?.cancel()
            pendingEndHold = workItem
            DispatchQueue.main.asyncAfter(
                deadline: .now() + gestureInterpreter.doubleTapWindow,
                execute: workItem
            )

        case .continueHandsFree:
            pendingEndHold?.cancel()
            pendingEndHold = nil
            onHandsFreeChanged?(true)

        case .endHoldNow, .endHandsFree:
            modifierOnlyCaptureBegan = false
            pendingEndHold?.cancel()
            pendingEndHold = nil
            onHandsFreeChanged?(false)
            onStopCapture?()

        case .cancel:
            modifierOnlyCaptureBegan = false
            pendingEndHold?.cancel()
            pendingEndHold = nil
            onHandsFreeChanged?(false)
            onCancelCapture?()
        }
    }

    private func handle(
        _ action: HotkeyGestureAction,
        forModifierOnlyHotkey: Bool
    ) {
        guard forModifierOnlyHotkey else {
            perform(action)
            return
        }

        switch action {
        case .beginHold:
            scheduleModifierOnlyBeginHold()

        case .scheduleEndHold, .endHoldNow:
            beginModifierOnlyCaptureIfNeeded()
            perform(action)

        default:
            perform(action)
        }
    }

    private func scheduleModifierOnlyBeginHold() {
        pendingBeginHold?.cancel()

        guard modifierOnlyActivationDelay > 0 else {
            beginModifierOnlyCaptureIfNeeded()
            return
        }

        let workItem = DispatchWorkItem { [weak self] in
            guard let self else { return }
            self.pendingBeginHold = nil
            self.beginModifierOnlyCaptureIfNeeded()
        }
        pendingBeginHold = workItem
        DispatchQueue.main.asyncAfter(
            deadline: .now() + modifierOnlyActivationDelay,
            execute: workItem
        )
    }

    private func beginModifierOnlyCaptureIfNeeded() {
        pendingBeginHold?.cancel()
        pendingBeginHold = nil
        guard !modifierOnlyCaptureBegan else { return }
        modifierOnlyCaptureBegan = true
        perform(.beginHold)
    }

    @discardableResult
    private func interruptModifierOnlyGesture() -> Bool {
        let hadPendingBegin = pendingBeginHold != nil
        let captureBegan = modifierOnlyCaptureBegan
        pendingBeginHold?.cancel()
        pendingBeginHold = nil
        isHandlingHotkeyKeyPress = false

        guard gestureInterpreter.cancel() != nil else { return false }

        if captureBegan {
            perform(.cancel)
        } else {
            modifierOnlyCaptureBegan = false
        }
        return hadPendingBegin || captureBegan
    }

    func reEnable() {
        if let port = port {
            CGEvent.tapEnable(tap: port, enable: true)
        }
    }

    /// Human-readable display string for the configured hotkey
    var displayString: String {
        AppSettings.hotkeyDisplayString(
            keyCode: hotkeyKeyCode,
            modifiers: hotkeyModifiers
        )
    }

    deinit {
        stop()
    }
}

// Free function callback required by CGEvent.tapCreate (cannot be a closure)
private func globalHotkeyCallback(
    proxy: CGEventTapProxy,
    type: CGEventType,
    event: CGEvent,
    refcon: UnsafeMutableRawPointer?
) -> Unmanaged<CGEvent>? {
    guard let refcon = refcon else { return Unmanaged.passUnretained(event) }
    let manager = Unmanaged<GlobalHotkeyManager>.fromOpaque(refcon).takeUnretainedValue()

    // Re-enable tap if macOS disabled it (timeout or user input flood)
    if type == .tapDisabledByTimeout || type == .tapDisabledByUserInput {
        manager.reEnable()
        return Unmanaged.passUnretained(event)
    }

    if type == .leftMouseDown || type == .rightMouseDown || type == .otherMouseDown {
        manager.resetTypedSnippetBuffer()
        manager.cancelModifierOnlyGesture()
        return Unmanaged.passUnretained(event)
    }

    let keyCode = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
    if manager.handleGestureEvent(type: type, event: event) {
        return nil
    }

    if type == .keyDown, keyCode == 53, manager.cancelGesture() {
        return nil
    }

    if type == .keyDown, manager.handleTypedSnippetEvent(event) {
        return nil
    }

    return Unmanaged.passUnretained(event) // pass through all other keys
}
