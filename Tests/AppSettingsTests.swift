import Testing
import Foundation
import CoreGraphics
@testable import VoiceScribeMac

@Suite("App Settings")
struct AppSettingsTests {
    let defaults: UserDefaults
    let settings: AppSettings

    init() {
        let suiteName = "test-\(UUID().uuidString)"
        defaults = UserDefaults(suiteName: suiteName)!
        settings = AppSettings(defaults: defaults)
    }

    // MARK: - Defaults

    @Test("Default language is 'auto'")
    func defaultLanguage() {
        #expect(settings.language == "auto")
    }

    @Test("Default noise reduction is OFF (low latency priority)")
    func defaultNoiseReduction() {
        #expect(settings.noiseReduction == false)
    }

    @Test("Default remove filler is true")
    func defaultRemoveFiller() {
        #expect(settings.removeFiller == true)
    }

    @Test("Default provider is automatic with cloud fallback disabled")
    func defaultProviderPrivacy() {
        #expect(settings.providerPreference == .automatic)
        #expect(settings.cloudRecognitionAllowed == false)
        #expect(settings.preferCloudForTechnicalSpeech == false)
        #expect(settings.requiresOnDeviceAppleSpeech == true)
    }

    @Test("Dictation is the default capture mode")
    func defaultCaptureMode() {
        #expect(settings.transcriptionMode == .dictation)
    }

    @Test("Capture mode persists")
    func captureModePersists() {
        settings.transcriptionMode = .command

        #expect(settings.transcriptionMode == .command)
    }

    @Test("Default Google configuration uses EU Chirp")
    func defaultGoogleConfiguration() {
        #expect(settings.googleCloudProjectID.isEmpty)
        #expect(settings.googleCloudLocation == "eu")
        #expect(settings.googleCloudModel == "chirp_3")
    }

    @Test("Failed audio retention is the private recovery default")
    func defaultRecoveryPrivacy() {
        #expect(settings.audioRetentionPolicy == .failures)
        #expect(settings.incognitoMode == false)
    }

    @Test("On-device faithful cleanup is explicit opt-in")
    func enhancementSettingPersists() {
        #expect(settings.faithfulEnhancementEnabled == false)
        #expect(settings.cleanupProviderID == AppleIntelligenceCleanupProvider.providerID)

        settings.faithfulEnhancementEnabled = true
        settings.cleanupProviderID = "  deterministic-fake  "

        #expect(settings.faithfulEnhancementEnabled)
        #expect(settings.cleanupProviderID == "deterministic-fake")
    }

    @Test("Per-application style memory is explicit opt-in")
    func styleMemorySettingPersists() {
        #expect(!settings.rememberLastStylePerApplication)

        settings.rememberLastStylePerApplication = true

        #expect(settings.rememberLastStylePerApplication)
    }

    @Test("Per-application mode and provider memory are explicit opt-ins")
    func applicationProfileSettingsPersist() {
        #expect(!settings.rememberLastModePerApplication)
        #expect(!settings.rememberProviderPerApplication)

        settings.rememberLastModePerApplication = true
        settings.rememberProviderPerApplication = true

        #expect(settings.rememberLastModePerApplication)
        #expect(settings.rememberProviderPerApplication)
    }

    @Test("Typed snippet expansion is explicit opt-in")
    func typedSnippetSettingPersists() {
        #expect(!settings.typedSnippetExpansionEnabled)

        settings.typedSnippetExpansionEnabled = true

        #expect(settings.typedSnippetExpansionEnabled)
    }

    @Test("Contextual rewriting requires global and application consent")
    func contextualRewritingConsentPersists() {
        let suiteName = "MluvaTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let settings = AppSettings(defaults: defaults)

        #expect(!settings.contextualFormattingEnabled)
        #expect(!settings.contextualFormattingAllowed(for: "com.apple.Notes"))

        settings.contextualFormattingEnabled = true
        #expect(settings.contextualFormattingAllowed(for: "com.apple.Notes"))

        settings.setContextualFormatting(false, for: "com.apple.mail")

        let reloaded = AppSettings(defaults: defaults)
        #expect(reloaded.contextualFormattingAllowed(for: "com.apple.Notes"))
        #expect(!reloaded.contextualFormattingAllowed(for: "com.apple.mail"))
    }

    @Test("History retention is configurable without changing existing installs")
    func historyRetentionPersists() {
        let suiteName = "MluvaTests.\(UUID().uuidString)"
        let defaults = UserDefaults(suiteName: suiteName)!
        defer { defaults.removePersistentDomain(forName: suiteName) }
        let settings = AppSettings(defaults: defaults)

        #expect(settings.historyRetentionDays == 0)

        settings.historyRetentionDays = 30

        #expect(AppSettings(defaults: defaults).historyRetentionDays == 30)
    }

    @Test("Recovery privacy settings persist")
    func recoveryPrivacyPersists() {
        settings.audioRetentionPolicy = .always
        settings.incognitoMode = true

        #expect(settings.audioRetentionPolicy == .always)
        #expect(settings.incognitoMode)
    }

    @Test("Default hasCompletedSetup is false")
    func defaultSetup() {
        #expect(settings.hasCompletedSetup == false)
    }

    // MARK: - Persistence

    @Test("Language persists to UserDefaults")
    func languagePersistence() {
        settings.language = "cs"
        #expect(settings.language == "cs")
        #expect(defaults.string(forKey: "voiceScribe.language") == "cs")
    }

    @Test("Noise reduction can be enabled")
    func noiseReductionPersistence() {
        settings.noiseReduction = true
        #expect(settings.noiseReduction == true)
    }

    @Test("Remove filler can be disabled")
    func removeFillerToggle() {
        settings.removeFiller = false
        #expect(settings.removeFiller == false)
    }

    @Test("Provider and privacy settings persist")
    func providerSettingsPersist() {
        settings.providerPreference = .googleCloud
        settings.cloudRecognitionAllowed = true
        settings.preferCloudForTechnicalSpeech = true
        settings.requiresOnDeviceAppleSpeech = false

        #expect(settings.providerPreference == .googleCloud)
        #expect(settings.cloudRecognitionAllowed)
        #expect(settings.preferCloudForTechnicalSpeech)
        #expect(!settings.requiresOnDeviceAppleSpeech)
    }

    @Test("Google project location and model persist")
    func googleSettingsPersist() {
        settings.googleCloudProjectID = "my-project"
        settings.googleCloudLocation = "us"
        settings.googleCloudModel = "chirp_2"
        settings.googleServiceAccountFilePath = "/private/service-account.json"

        #expect(settings.googleCloudProjectID == "my-project")
        #expect(settings.googleCloudLocation == "us")
        #expect(settings.googleCloudModel == "chirp_2")
        #expect(settings.googleServiceAccountFilePath == "/private/service-account.json")
    }

    @Test("hasCompletedSetup persists")
    func setupPersistence() {
        settings.hasCompletedSetup = true
        #expect(settings.hasCompletedSetup == true)
    }

    // MARK: - Hotkey Defaults

    @Test("Default hotkey key code is Right Command")
    func defaultHotkeyKeyCode() {
        #expect(settings.hotkeyKeyCode == 0x36)
    }

    @Test("Default hotkey modifier is Command")
    func defaultHotkeyModifiers() {
        #expect(settings.hotkeyModifiers == CGEventFlags.maskCommand.rawValue)
    }

    @Test("Default hotkey display string distinguishes Right Command")
    func defaultHotkeyDisplayString() {
        #expect(settings.hotkeyDisplayString == "Right \u{2318}")
    }

    // MARK: - Hotkey Persistence

    @Test("Hotkey key code persists")
    func hotkeyKeyCodePersistence() {
        settings.hotkeyKeyCode = 15 // R
        #expect(settings.hotkeyKeyCode == 15)
        #expect(defaults.integer(forKey: "voiceScribe.hotkeyKeyCode") == 15)
    }

    @Test("Hotkey modifiers persist")
    func hotkeyModifiersPersistence() {
        let newMods = CGEventFlags.maskCommand.rawValue | CGEventFlags.maskShift.rawValue
        settings.hotkeyModifiers = newMods
        #expect(settings.hotkeyModifiers == newMods)
    }

    @Test("Hotkey modifier flags computed property works")
    func hotkeyModifierFlagsProperty() {
        settings.hotkeyModifierFlags = CGEventFlags([.maskAlternate, .maskShift])
        let flags = settings.hotkeyModifierFlags
        #expect(flags.contains(.maskAlternate))
        #expect(flags.contains(.maskShift))
        #expect(!flags.contains(.maskCommand))
        #expect(!flags.contains(.maskControl))
    }

    @Test("Hotkey display string updates after change")
    func hotkeyDisplayStringAfterChange() {
        settings.hotkeyKeyCode = 1  // S
        settings.hotkeyModifierFlags = .maskAlternate  // Option
        #expect(settings.hotkeyDisplayString == "\u{2325}S")
    }

    @Test("Reset hotkey to default clears stored values")
    func resetHotkeyToDefault() {
        // Set custom hotkey
        settings.hotkeyKeyCode = 15
        settings.hotkeyModifiers = CGEventFlags.maskCommand.rawValue
        #expect(settings.hotkeyKeyCode == 15)

        // Reset
        settings.resetHotkeyToDefault()
        #expect(settings.hotkeyKeyCode == AppSettings.defaultHotkeyKeyCode)
        #expect(settings.hotkeyModifiers == AppSettings.defaultHotkeyModifiers)
    }

    // MARK: - Key Name Mapping

    @Test("Key name for known keys")
    func keyNameMapping() {
        #expect(AppSettings.keyName(for: 0) == "A")
        #expect(AppSettings.keyName(for: 1) == "S")
        #expect(AppSettings.keyName(for: 9) == "V")
        #expect(AppSettings.keyName(for: 15) == "R")
        #expect(AppSettings.keyName(for: 49) == "\u{2423}") // Space
        #expect(AppSettings.keyName(for: 54) == "Right \u{2318}")
    }

    @Test("Physical modifier keys are recognized independently")
    func modifierOnlyHotkeys() {
        #expect(AppSettings.modifierFlag(for: 54) == .maskCommand)
        #expect(AppSettings.modifierFlag(for: 55) == .maskCommand)
        #expect(AppSettings.modifierFlag(for: 63) == .maskSecondaryFn)
        #expect(AppSettings.modifierFlag(for: 9) == nil)
        #expect(AppSettings.isModifierOnlyHotkey(keyCode: 54, modifiers: .maskCommand))
        #expect(!AppSettings.isModifierOnlyHotkey(
            keyCode: 54,
            modifiers: [.maskCommand, .maskShift]
        ))
    }

    @Test("Key name for unknown key code")
    func keyNameUnknown() {
        #expect(AppSettings.keyName(for: 200) == "Key200")
    }
}
