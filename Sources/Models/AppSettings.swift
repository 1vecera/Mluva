import Foundation
import CoreGraphics

final class AppSettings {
    static let shared = AppSettings()

    private let defaults: UserDefaults

    init(defaults: UserDefaults = .standard) {
        self.defaults = defaults
    }

    private enum Keys {
        static let language = "voiceScribe.language"
        static let noiseReduction = "voiceScribe.noiseReduction"
        static let removeFiller = "voiceScribe.removeFiller"
        static let hasCompletedSetup = "voiceScribe.hasCompletedSetup"
        static let hotkeyKeyCode = "voiceScribe.hotkeyKeyCode"
        static let hotkeyModifiers = "voiceScribe.hotkeyModifiers"
        static let providerPreference = "voiceScribe.providerPreference"
        static let transcriptionMode = "voiceScribe.transcriptionMode"
        static let cloudRecognitionAllowed = "voiceScribe.cloudRecognitionAllowed"
        static let preferCloudForTechnicalSpeech = "voiceScribe.preferCloudForTechnicalSpeech"
        static let requiresOnDeviceAppleSpeech = "voiceScribe.requiresOnDeviceAppleSpeech"
        static let googleCloudProjectID = "voiceScribe.googleCloudProjectID"
        static let googleCloudLocation = "voiceScribe.googleCloudLocation"
        static let googleCloudModel = "voiceScribe.googleCloudModel"
        static let googleServiceAccountFilePath = "voiceScribe.googleServiceAccountFilePath"
        static let audioRetentionPolicy = "voiceScribe.audioRetentionPolicy"
        static let incognitoMode = "voiceScribe.incognitoMode"
        static let faithfulEnhancementEnabled = "voiceScribe.faithfulEnhancementEnabled"
        static let cleanupProviderID = "voiceScribe.cleanupProviderID"
        static let rememberLastStylePerApplication = "voiceScribe.rememberLastStylePerApplication"
        static let rememberLastModePerApplication = "voiceScribe.rememberLastModePerApplication"
        static let rememberProviderPerApplication = "voiceScribe.rememberProviderPerApplication"
        static let contextualFormattingEnabled = "voiceScribe.contextualFormattingEnabled"
        static let contextDisabledBundleIdentifiers = "voiceScribe.contextDisabledBundleIdentifiers"
        static let historyRetentionDays = "voiceScribe.historyRetentionDays"
        static let typedSnippetExpansionEnabled = "voiceScribe.typedSnippetExpansionEnabled"
    }

    // MARK: - Hotkey Configuration

    /// Default hotkey: Right Command. Modifier-only shortcuts use the physical
    /// modifier key code so left and right variants remain distinct.
    static let defaultHotkeyKeyCode: UInt16 = 0x36
    static let defaultHotkeyModifiers: UInt64 = CGEventFlags.maskCommand.rawValue

    var hotkeyKeyCode: UInt16 {
        get {
            let stored = defaults.object(forKey: Keys.hotkeyKeyCode)
            if stored == nil { return Self.defaultHotkeyKeyCode }
            return UInt16(defaults.integer(forKey: Keys.hotkeyKeyCode))
        }
        set { defaults.set(Int(newValue), forKey: Keys.hotkeyKeyCode) }
    }

    var hotkeyModifiers: UInt64 {
        get {
            let stored = defaults.object(forKey: Keys.hotkeyModifiers)
            if stored == nil { return Self.defaultHotkeyModifiers }
            return UInt64(defaults.integer(forKey: Keys.hotkeyModifiers))
        }
        set { defaults.set(Int(newValue), forKey: Keys.hotkeyModifiers) }
    }

    var hotkeyModifierFlags: CGEventFlags {
        get { CGEventFlags(rawValue: hotkeyModifiers) }
        set { hotkeyModifiers = newValue.rawValue }
    }

    /// Human-readable display string for the current hotkey combo.
    var hotkeyDisplayString: String {
        Self.hotkeyDisplayString(
            keyCode: hotkeyKeyCode,
            modifiers: hotkeyModifierFlags
        )
    }

    static func hotkeyDisplayString(
        keyCode: UInt16,
        modifiers: CGEventFlags
    ) -> String {
        if isModifierOnlyHotkey(keyCode: keyCode, modifiers: modifiers) {
            return modifierKeyName(for: keyCode) ?? keyName(for: keyCode)
        }

        var parts: [String] = []
        if modifiers.contains(.maskSecondaryFn) { parts.append("Fn") }
        if modifiers.contains(.maskControl) { parts.append("\u{2303}") }
        if modifiers.contains(.maskAlternate) { parts.append("\u{2325}") }
        if modifiers.contains(.maskShift) { parts.append("\u{21E7}") }
        if modifiers.contains(.maskCommand) { parts.append("\u{2318}") }
        parts.append(keyName(for: keyCode))
        return parts.joined()
    }

    static func modifierFlag(for keyCode: UInt16) -> CGEventFlags? {
        switch keyCode {
        case 0x36, 0x37:
            return .maskCommand
        case 0x38, 0x3C:
            return .maskShift
        case 0x3A, 0x3D:
            return .maskAlternate
        case 0x3B, 0x3E:
            return .maskControl
        case 0x3F:
            return .maskSecondaryFn
        default:
            return nil
        }
    }

    static func isModifierOnlyHotkey(
        keyCode: UInt16,
        modifiers: CGEventFlags
    ) -> Bool {
        guard let modifier = modifierFlag(for: keyCode) else { return false }
        return modifiers.intersection(supportedHotkeyModifiers) == modifier
    }

    static let supportedHotkeyModifiers: CGEventFlags = [
        .maskCommand,
        .maskControl,
        .maskAlternate,
        .maskShift,
        .maskSecondaryFn,
    ]

    private static func modifierKeyName(for keyCode: UInt16) -> String? {
        switch keyCode {
        case 0x36: return "Right \u{2318}"
        case 0x37: return "Left \u{2318}"
        case 0x38: return "Left \u{21E7}"
        case 0x3A: return "Left \u{2325}"
        case 0x3B: return "Left \u{2303}"
        case 0x3C: return "Right \u{21E7}"
        case 0x3D: return "Right \u{2325}"
        case 0x3E: return "Right \u{2303}"
        case 0x3F: return "Fn"
        default: return nil
        }
    }

    func resetHotkeyToDefault() {
        defaults.removeObject(forKey: Keys.hotkeyKeyCode)
        defaults.removeObject(forKey: Keys.hotkeyModifiers)
    }

    /// Map common virtual key codes to display names
    static func keyName(for keyCode: UInt16) -> String {
        switch keyCode {
        case 0:  return "A"
        case 1:  return "S"
        case 2:  return "D"
        case 3:  return "F"
        case 4:  return "H"
        case 5:  return "G"
        case 6:  return "Z"
        case 7:  return "X"
        case 8:  return "C"
        case 9:  return "V"
        case 10: return "\u{00A7}" // section sign (ISO keyboards)
        case 11: return "B"
        case 12: return "Q"
        case 13: return "W"
        case 14: return "E"
        case 15: return "R"
        case 16: return "Y"
        case 17: return "T"
        case 18: return "1"
        case 19: return "2"
        case 20: return "3"
        case 21: return "4"
        case 22: return "6"
        case 23: return "5"
        case 24: return "="
        case 25: return "9"
        case 26: return "7"
        case 27: return "-"
        case 28: return "8"
        case 29: return "0"
        case 30: return "]"
        case 31: return "O"
        case 32: return "U"
        case 33: return "["
        case 34: return "I"
        case 35: return "P"
        case 36: return "\u{21A9}" // Return
        case 37: return "L"
        case 38: return "J"
        case 39: return "'"
        case 40: return "K"
        case 41: return ";"
        case 42: return "\\"
        case 43: return ","
        case 44: return "/"
        case 45: return "N"
        case 46: return "M"
        case 47: return "."
        case 48: return "\u{21E5}" // Tab
        case 49: return "\u{2423}" // Space
        case 50: return "`"
        case 51: return "\u{232B}" // Delete
        case 53: return "\u{238B}" // Escape
        case 54: return "Right \u{2318}"
        case 55: return "Left \u{2318}"
        case 56: return "Left \u{21E7}"
        case 58: return "Left \u{2325}"
        case 59: return "Left \u{2303}"
        case 60: return "Right \u{21E7}"
        case 61: return "Right \u{2325}"
        case 62: return "Right \u{2303}"
        case 63: return "Fn"
        case 96:  return "F5"
        case 97:  return "F6"
        case 98:  return "F7"
        case 99:  return "F3"
        case 100: return "F8"
        case 101: return "F9"
        case 103: return "F11"
        case 105: return "F13"
        case 107: return "F14"
        case 109: return "F10"
        case 111: return "F12"
        case 113: return "F15"
        case 118: return "F4"
        case 120: return "F2"
        case 122: return "F1"
        case 123: return "\u{2190}" // Left
        case 124: return "\u{2192}" // Right
        case 125: return "\u{2193}" // Down
        case 126: return "\u{2191}" // Up
        default: return "Key\(keyCode)"
        }
    }

    var language: String {
        get { defaults.string(forKey: Keys.language) ?? "auto" }
        set { defaults.set(newValue, forKey: Keys.language) }
    }

    var providerPreference: TranscriptionProviderKind {
        get {
            let rawValue = defaults.string(forKey: Keys.providerPreference) ?? "automatic"
            return TranscriptionProviderKind(rawValue: rawValue) ?? .automatic
        }
        set { defaults.set(newValue.rawValue, forKey: Keys.providerPreference) }
    }

    var transcriptionMode: TranscriptionMode {
        get {
            let rawValue = defaults.string(forKey: Keys.transcriptionMode) ?? "dictation"
            return TranscriptionMode(rawValue: rawValue) ?? .dictation
        }
        set { defaults.set(newValue.rawValue, forKey: Keys.transcriptionMode) }
    }

    var cloudRecognitionAllowed: Bool {
        get { defaults.bool(forKey: Keys.cloudRecognitionAllowed) }
        set { defaults.set(newValue, forKey: Keys.cloudRecognitionAllowed) }
    }

    var preferCloudForTechnicalSpeech: Bool {
        get { defaults.bool(forKey: Keys.preferCloudForTechnicalSpeech) }
        set { defaults.set(newValue, forKey: Keys.preferCloudForTechnicalSpeech) }
    }

    var requiresOnDeviceAppleSpeech: Bool {
        get {
            if defaults.object(forKey: Keys.requiresOnDeviceAppleSpeech) == nil { return true }
            return defaults.bool(forKey: Keys.requiresOnDeviceAppleSpeech)
        }
        set { defaults.set(newValue, forKey: Keys.requiresOnDeviceAppleSpeech) }
    }

    var googleCloudProjectID: String {
        get { defaults.string(forKey: Keys.googleCloudProjectID) ?? "" }
        set { defaults.set(newValue.trimmingCharacters(in: .whitespacesAndNewlines), forKey: Keys.googleCloudProjectID) }
    }

    var googleCloudLocation: String {
        get { defaults.string(forKey: Keys.googleCloudLocation) ?? "eu" }
        set { defaults.set(newValue.trimmingCharacters(in: .whitespacesAndNewlines), forKey: Keys.googleCloudLocation) }
    }

    var googleCloudModel: String {
        get { defaults.string(forKey: Keys.googleCloudModel) ?? "chirp_3" }
        set { defaults.set(newValue.trimmingCharacters(in: .whitespacesAndNewlines), forKey: Keys.googleCloudModel) }
    }

    var googleServiceAccountFilePath: String {
        get { defaults.string(forKey: Keys.googleServiceAccountFilePath) ?? "" }
        set {
            defaults.set(
                newValue.trimmingCharacters(in: .whitespacesAndNewlines),
                forKey: Keys.googleServiceAccountFilePath
            )
        }
    }

    var cloudFallbackAvailable: Bool {
        cloudRecognitionAllowed && !googleCloudProjectID.isEmpty
    }

    var audioRetentionPolicy: AudioRetentionPolicy {
        get {
            let rawValue = defaults.string(forKey: Keys.audioRetentionPolicy) ?? "failures"
            return AudioRetentionPolicy(rawValue: rawValue) ?? .failures
        }
        set { defaults.set(newValue.rawValue, forKey: Keys.audioRetentionPolicy) }
    }

    var incognitoMode: Bool {
        get { defaults.bool(forKey: Keys.incognitoMode) }
        set { defaults.set(newValue, forKey: Keys.incognitoMode) }
    }

    var faithfulEnhancementEnabled: Bool {
        get { defaults.bool(forKey: Keys.faithfulEnhancementEnabled) }
        set { defaults.set(newValue, forKey: Keys.faithfulEnhancementEnabled) }
    }

    var cleanupProviderID: String {
        get {
            defaults.string(forKey: Keys.cleanupProviderID)
                ?? AppleIntelligenceCleanupProvider.providerID
        }
        set {
            defaults.set(
                newValue.trimmingCharacters(in: .whitespacesAndNewlines),
                forKey: Keys.cleanupProviderID
            )
        }
    }

    var rememberLastStylePerApplication: Bool {
        get { defaults.bool(forKey: Keys.rememberLastStylePerApplication) }
        set { defaults.set(newValue, forKey: Keys.rememberLastStylePerApplication) }
    }

    var rememberLastModePerApplication: Bool {
        get { defaults.bool(forKey: Keys.rememberLastModePerApplication) }
        set { defaults.set(newValue, forKey: Keys.rememberLastModePerApplication) }
    }

    var rememberProviderPerApplication: Bool {
        get { defaults.bool(forKey: Keys.rememberProviderPerApplication) }
        set { defaults.set(newValue, forKey: Keys.rememberProviderPerApplication) }
    }

    var contextualFormattingEnabled: Bool {
        get { defaults.bool(forKey: Keys.contextualFormattingEnabled) }
        set { defaults.set(newValue, forKey: Keys.contextualFormattingEnabled) }
    }

    func contextualFormattingAllowed(for bundleIdentifier: String?) -> Bool {
        guard contextualFormattingEnabled else { return false }
        guard let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier) else {
            return true
        }
        return !contextDisabledBundleIdentifiers.contains(bundleIdentifier)
    }

    func setContextualFormatting(_ enabled: Bool, for bundleIdentifier: String) {
        guard let bundleIdentifier = normalizedBundleIdentifier(bundleIdentifier) else { return }
        var identifiers = contextDisabledBundleIdentifiers
        if enabled {
            identifiers.remove(bundleIdentifier)
        } else {
            identifiers.insert(bundleIdentifier)
        }
        defaults.set(identifiers.sorted(), forKey: Keys.contextDisabledBundleIdentifiers)
    }

    private var contextDisabledBundleIdentifiers: Set<String> {
        Set(defaults.stringArray(forKey: Keys.contextDisabledBundleIdentifiers) ?? [])
    }

    private func normalizedBundleIdentifier(_ value: String?) -> String? {
        guard let value else { return nil }
        let normalized = value.trimmingCharacters(in: .whitespacesAndNewlines)
        return normalized.isEmpty ? nil : normalized
    }

    var historyRetentionDays: Int {
        get { max(0, defaults.integer(forKey: Keys.historyRetentionDays)) }
        set { defaults.set(max(0, newValue), forKey: Keys.historyRetentionDays) }
    }

    var typedSnippetExpansionEnabled: Bool {
        get { defaults.bool(forKey: Keys.typedSnippetExpansionEnabled) }
        set { defaults.set(newValue, forKey: Keys.typedSnippetExpansionEnabled) }
    }

    /// Noise reduction OFF by default — prioritize low latency over audio quality
    var noiseReduction: Bool {
        get { defaults.bool(forKey: Keys.noiseReduction) }
        set { defaults.set(newValue, forKey: Keys.noiseReduction) }
    }

    var removeFiller: Bool {
        get {
            if defaults.object(forKey: Keys.removeFiller) == nil { return true }
            return defaults.bool(forKey: Keys.removeFiller)
        }
        set { defaults.set(newValue, forKey: Keys.removeFiller) }
    }

    var hasCompletedSetup: Bool {
        get { defaults.bool(forKey: Keys.hasCompletedSetup) }
        set { defaults.set(newValue, forKey: Keys.hasCompletedSetup) }
    }
}
