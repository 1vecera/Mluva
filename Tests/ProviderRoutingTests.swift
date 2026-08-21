import Testing
@testable import VoiceScribeMac

@Suite("Transcription provider routing")
struct ProviderRoutingTests {
    private let router = TranscriptionProviderRouter()

    @Test("Automatic routing prefers private Apple recognition")
    func automaticPrefersApple() throws {
        let request = ProviderRoutingRequest(
            preference: .automatic,
            language: "en-US",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US", "cs-CZ"],
            googleCloudConfigured: true,
            cloudAllowed: true
        )

        #expect(try router.resolve(request, capabilities: capabilities) == .apple)
    }

    @Test("Automatic routing uses Google for requested technical accuracy")
    func automaticUsesGoogleForTechnicalSpeech() throws {
        let request = ProviderRoutingRequest(
            preference: .automatic,
            language: "en-US",
            needsCloudAccuracy: true
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            googleCloudConfigured: true,
            cloudAllowed: true
        )

        #expect(try router.resolve(request, capabilities: capabilities) == .googleCloud)
    }

    @Test("Automatic routing falls back to Google when Apple Speech is not authorized")
    func automaticUsesGoogleWithoutAppleAuthorization() throws {
        let request = ProviderRoutingRequest(
            preference: .automatic,
            language: "en-US",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            appleRecognitionAuthorized: false,
            googleCloudConfigured: true,
            cloudAllowed: true
        )

        #expect(try router.resolve(request, capabilities: capabilities) == .googleCloud)
    }

    @Test("Explicit Apple selection reports missing Speech authorization")
    func explicitAppleRequiresAuthorization() {
        let request = ProviderRoutingRequest(
            preference: .apple,
            language: "en-US",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            appleRecognitionAuthorized: false,
            googleCloudConfigured: true,
            cloudAllowed: true
        )

        #expect(throws: ProviderRoutingError.applePermissionRequired) {
            try router.resolve(request, capabilities: capabilities)
        }
    }

    @Test("Privacy policy prevents automatic cloud fallback")
    func privacyPreventsCloudFallback() {
        let request = ProviderRoutingRequest(
            preference: .automatic,
            language: "pl-PL",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            googleCloudConfigured: true,
            cloudAllowed: false
        )

        #expect(throws: ProviderRoutingError.noPermittedProvider) {
            try router.resolve(request, capabilities: capabilities)
        }
    }

    @Test("Explicit Google selection reports missing configuration")
    func explicitGoogleRequiresConfiguration() {
        let request = ProviderRoutingRequest(
            preference: .googleCloud,
            language: "en-US",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            googleCloudConfigured: false,
            cloudAllowed: true
        )

        #expect(throws: ProviderRoutingError.googleCloudNotConfigured) {
            try router.resolve(request, capabilities: capabilities)
        }
    }

    @Test("Explicit Apple selection reports unsupported language")
    func explicitAppleRequiresLanguageSupport() {
        let request = ProviderRoutingRequest(
            preference: .apple,
            language: "pl-PL",
            needsCloudAccuracy: false
        )
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: ["en-US"],
            googleCloudConfigured: true,
            cloudAllowed: true
        )

        #expect(throws: ProviderRoutingError.appleLanguageUnavailable("pl-PL")) {
            try router.resolve(request, capabilities: capabilities)
        }
    }
}
