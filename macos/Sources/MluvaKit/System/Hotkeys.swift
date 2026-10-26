import Carbon.HIToolbox
import Foundation

/// The function-key names Mluva's settings use, with their macOS virtual key codes.
public enum FunctionKey {
    static let codes: [String: UInt32] = [
        "F1": UInt32(kVK_F1), "F2": UInt32(kVK_F2), "F3": UInt32(kVK_F3), "F4": UInt32(kVK_F4),
        "F5": UInt32(kVK_F5), "F6": UInt32(kVK_F6), "F7": UInt32(kVK_F7), "F8": UInt32(kVK_F8),
        "F9": UInt32(kVK_F9), "F10": UInt32(kVK_F10), "F11": UInt32(kVK_F11), "F12": UInt32(kVK_F12),
        "F13": UInt32(kVK_F13), "F14": UInt32(kVK_F14), "F15": UInt32(kVK_F15), "F16": UInt32(kVK_F16),
        "F17": UInt32(kVK_F17), "F18": UInt32(kVK_F18), "F19": UInt32(kVK_F19), "F20": UInt32(kVK_F20),
    ]

    /// F1–F20 exist on Mac keyboards; the shared settings also allow up to F24 for Linux.
    public static let names: [String] = (1...20).map { "F\($0)" }

    public static func code(_ name: String) -> UInt32? { codes[name] }
}

/// What a global shortcut does.
public enum HotkeyAction: String, Sendable, CaseIterable {
    case dictate
    case agent
    case keepDraft
}

/// System-wide function-key shortcuts through Carbon hot keys. They need no Accessibility or
/// Input Monitoring permission, receive only their own key, and never see other typing.
@MainActor
public final class HotkeyCenter {
    public var onPress: ((HotkeyAction) -> Void)?

    private var registered: [HotkeyAction: EventHotKeyRef] = [:]
    private var handler: EventHandlerRef?
    private static let signature: OSType = 0x4D4C_5641 // "MLVA"

    public init() {}

    /// Replace every binding. Returns the actions whose key macOS refused, such as a key
    /// another application already owns.
    @discardableResult
    public func bind(_ keys: [HotkeyAction: String]) -> [HotkeyAction] {
        installHandler()
        unbindAll()
        var refused: [HotkeyAction] = []
        for (action, name) in keys {
            guard let code = FunctionKey.code(name) else {
                refused.append(action)
                continue
            }
            let id = EventHotKeyID(signature: Self.signature, id: UInt32(HotkeyAction.allCases.firstIndex(of: action)!))
            var reference: EventHotKeyRef?
            let status = RegisterEventHotKey(code, 0, id, GetApplicationEventTarget(), 0, &reference)
            if status == noErr, let reference {
                registered[action] = reference
            } else {
                refused.append(action)
            }
        }
        return refused
    }

    public func unbindAll() {
        for reference in registered.values { UnregisterEventHotKey(reference) }
        registered.removeAll()
    }

    private func installHandler() {
        guard handler == nil else { return }
        var spec = EventTypeSpec(eventClass: OSType(kEventClassKeyboard), eventKind: UInt32(kEventHotKeyPressed))
        let context = Unmanaged.passUnretained(self).toOpaque()
        InstallEventHandler(GetApplicationEventTarget(), { _, event, context in
            guard let event, let context else { return OSStatus(eventNotHandledErr) }
            var id = EventHotKeyID()
            let status = GetEventParameter(
                event, EventParamName(kEventParamDirectObject), EventParamType(typeEventHotKeyID),
                nil, MemoryLayout<EventHotKeyID>.size, nil, &id)
            guard status == noErr, id.signature == HotkeyCenter.signature,
                  Int(id.id) < HotkeyAction.allCases.count else { return OSStatus(eventNotHandledErr) }
            let action = HotkeyAction.allCases[Int(id.id)]
            let center = Unmanaged<HotkeyCenter>.fromOpaque(context).takeUnretainedValue()
            MainActor.assumeIsolated { center.onPress?(action) }
            return noErr
        }, 1, &spec, context, &handler)
    }
}
