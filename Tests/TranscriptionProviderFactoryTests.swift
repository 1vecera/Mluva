import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Transcription provider factory")
struct TranscriptionProviderFactoryTests {
    @Test("Automatic settings build Apple when on-device language is available")
    func buildsAppleAutomatically() throws {
        let settings = makeSettings()
        settings.providerPreference = .automatic
        settings.language = "en"
        settings.cloudRecognitionAllowed = true
        settings.googleCloudProjectID = "example-project"

        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US", "cs-CZ"] },
            appleRecognitionAuthorized: { true }
        )
        let provider = try factory.makeProvider(settings: settings)

        #expect(provider.kind == .apple)
    }

    @Test("Technical accuracy setting builds Google when cloud is permitted")
    func buildsGoogleForTechnicalSpeech() throws {
        let settings = makeSettings()
        settings.providerPreference = .automatic
        settings.language = "en"
        settings.cloudRecognitionAllowed = true
        settings.preferCloudForTechnicalSpeech = true
        settings.googleCloudProjectID = "example-project"

        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true }
        )
        let provider = try factory.makeProvider(settings: settings)

        #expect(provider.kind == .googleCloud)
    }

    @Test("Google cannot be selected without explicit cloud permission")
    func googleRequiresCloudPermission() {
        let settings = makeSettings()
        settings.providerPreference = .googleCloud
        settings.googleCloudProjectID = "example-project"
        settings.cloudRecognitionAllowed = false

        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true }
        )

        #expect(throws: ProviderRoutingError.cloudNotPermitted) {
            try factory.makeProvider(settings: settings)
        }
    }

    @Test("Application provider override routes without mutating global preference")
    func routesApplicationProviderOverride() throws {
        let settings = makeSettings()
        settings.providerPreference = .apple
        settings.cloudRecognitionAllowed = true
        settings.googleCloudProjectID = "example-project"
        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true }
        )

        let provider = try factory.makeProvider(
            settings: settings,
            preference: .googleCloud
        )

        #expect(provider.kind == .googleCloud)
        #expect(settings.providerPreference == .apple)
    }

    @Test("Application provider override cannot bypass cloud consent")
    func applicationProviderOverrideRequiresCloudPermission() {
        let settings = makeSettings()
        settings.providerPreference = .apple
        settings.googleCloudProjectID = "example-project"
        settings.cloudRecognitionAllowed = false
        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true }
        )

        #expect(throws: ProviderRoutingError.cloudNotPermitted) {
            try factory.makeProvider(settings: settings, preference: .googleCloud)
        }
    }

    @Test("Fallback provider obeys cloud consent")
    func fallbackProviderObeysCloudConsent() throws {
        let settings = makeSettings()
        settings.googleCloudProjectID = "example-project"
        settings.cloudRecognitionAllowed = false
        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true }
        )

        #expect(try factory.makeFallbackProvider(
            settings: settings,
            excluding: .apple
        ) == nil)

        settings.cloudRecognitionAllowed = true
        let fallback = try factory.makeFallbackProvider(
            settings: settings,
            excluding: .apple
        )

        #expect(fallback?.kind == .googleCloud)
    }

    @Test("Language catalog supplies provider-specific locale identifiers")
    func mapsProviderLocales() {
        #expect(TranscriptionLanguage(identifier: "en").appleLocaleIdentifier == "en-US")
        #expect(TranscriptionLanguage(identifier: "cs").googleLanguageCode == "cs-CZ")
        #expect(TranscriptionLanguage(identifier: "pl-PL").appleLocaleIdentifier == "pl-PL")
        #expect(
            TranscriptionLanguage(
                identifier: "en_US@rg=czzzzz"
            ).googleLanguageCode == "en-US"
        )
    }

    @Test("Apple receives persisted vocabulary as recognition context")
    func suppliesAppleVocabulary() async throws {
        let settings = makeSettings()
        settings.providerPreference = .apple
        settings.language = "en"
        let engine = FactoryAppleSpeechEngine()
        let personalization = PersonalizationStore(
            fileURL: FileManager.default.temporaryDirectory
                .appendingPathComponent("factory-personalization-\(UUID().uuidString).json")
        )
        personalization.saveDictionaryReplacement(spoken: "post grass", written: "Postgres")
        personalization.saveSnippet(trigger: "email signoff", expansion: "Best, Daniel")
        let factory = DefaultTranscriptionProviderFactory(
            appleOnDeviceLanguages: { ["en-US"] },
            appleRecognitionAuthorized: { true },
            makeAppleEngine: { engine },
            personalizationStore: personalization
        )

        let provider = try factory.makeProvider(settings: settings)
        try await provider.start()

        #expect(engine.contextualStrings == [
            "post grass",
            "Postgres",
            "email signoff",
        ])
    }

    private func makeSettings() -> AppSettings {
        AppSettings(defaults: UserDefaults(suiteName: "factory-\(UUID().uuidString)")!)
    }
}

private final class FactoryAppleSpeechEngine: AppleSpeechEngine {
    var onRecognition: ((String, Bool) -> Void)?
    private(set) var contextualStrings: [String] = []

    func start(
        localeIdentifier: String,
        requiresOnDeviceRecognition: Bool,
        contextualStrings: [String]
    ) async throws {
        self.contextualStrings = contextualStrings
    }

    func appendPCM16(_ data: Data) throws {}
    func finish() async throws {}
    func cancel() {}
}
