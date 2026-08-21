import Foundation
import Speech

protocol TranscriptionProviderBuilding {
    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider
    func makeProvider(
        settings: AppSettings,
        preference: TranscriptionProviderKind?
    ) throws -> any TranscriptionProvider
    func makeFallbackProvider(
        settings: AppSettings,
        excluding provider: TranscriptionProviderKind
    ) throws -> (any TranscriptionProvider)?
}

extension TranscriptionProviderBuilding {
    func makeProvider(
        settings: AppSettings,
        preference: TranscriptionProviderKind?
    ) throws -> any TranscriptionProvider {
        try makeProvider(settings: settings)
    }

    func makeFallbackProvider(
        settings: AppSettings,
        excluding provider: TranscriptionProviderKind
    ) throws -> (any TranscriptionProvider)? {
        nil
    }
}

struct DefaultTranscriptionProviderFactory: TranscriptionProviderBuilding {
    private let router: TranscriptionProviderRouter
    private let appleOnDeviceLanguages: () -> Set<String>
    private let appleRecognitionAuthorized: () -> Bool
    private let makeAppleEngine: () -> any AppleSpeechEngine
    private let personalizationStore: PersonalizationStore
    private let tokenProvider: (any GoogleAccessTokenProviding)?
    private let tokenProviderFactory: GoogleAccessTokenProviderFactory
    private let transport: any HTTPDataTransport
    private let googleStreamingTransport: (any GoogleStreamingRecognitionTransport)?

    init(
        router: TranscriptionProviderRouter = TranscriptionProviderRouter(),
        appleOnDeviceLanguages: @escaping () -> Set<String> = Self.systemAppleOnDeviceLanguages,
        appleRecognitionAuthorized: @escaping () -> Bool = {
            SFSpeechRecognizer.authorizationStatus() == .authorized
        },
        makeAppleEngine: @escaping () -> any AppleSpeechEngine = { SystemAppleSpeechEngine() },
        personalizationStore: PersonalizationStore = .shared,
        tokenProvider: (any GoogleAccessTokenProviding)? = nil,
        tokenProviderFactory: GoogleAccessTokenProviderFactory = GoogleAccessTokenProviderFactory(),
        transport: any HTTPDataTransport = URLSessionHTTPDataTransport(),
        googleStreamingTransport: (any GoogleStreamingRecognitionTransport)? = Self.systemGoogleStreamingTransport()
    ) {
        self.router = router
        self.appleOnDeviceLanguages = appleOnDeviceLanguages
        self.appleRecognitionAuthorized = appleRecognitionAuthorized
        self.makeAppleEngine = makeAppleEngine
        self.personalizationStore = personalizationStore
        self.tokenProvider = tokenProvider
        self.tokenProviderFactory = tokenProviderFactory
        self.transport = transport
        self.googleStreamingTransport = googleStreamingTransport
    }

    func makeProvider(settings: AppSettings) throws -> any TranscriptionProvider {
        try makeProvider(settings: settings, preference: nil)
    }

    func makeProvider(
        settings: AppSettings,
        preference: TranscriptionProviderKind?
    ) throws -> any TranscriptionProvider {
        let language = TranscriptionLanguage(identifier: settings.language)
        let capabilities = ProviderCapabilities(
            appleOnDeviceLanguages: appleOnDeviceLanguages(),
            appleRecognitionAuthorized: appleRecognitionAuthorized(),
            googleCloudConfigured: !settings.googleCloudProjectID.isEmpty,
            cloudAllowed: settings.cloudRecognitionAllowed
        )
        let request = ProviderRoutingRequest(
            preference: preference ?? settings.providerPreference,
            language: language.appleLocaleIdentifier,
            needsCloudAccuracy: settings.preferCloudForTechnicalSpeech
        )

        switch try router.resolve(request, capabilities: capabilities) {
        case .apple:
            return AppleSpeechTranscriptionProvider(
                configuration: AppleSpeechConfiguration(
                    localeIdentifier: language.appleLocaleIdentifier,
                    requiresOnDeviceRecognition: settings.requiresOnDeviceAppleSpeech,
                    contextualStrings: personalizationStore.recognitionContext
                ),
                engine: makeAppleEngine()
            )

        case .googleCloud:
            return GoogleCloudTranscriptionProvider(
                configuration: GoogleCloudSpeechConfiguration(
                    projectID: settings.googleCloudProjectID,
                    location: settings.googleCloudLocation,
                    model: settings.googleCloudModel,
                    languageCodes: [language.googleLanguageCode]
                ),
                tokenProvider: tokenProvider ?? tokenProviderFactory.makeProvider(
                    serviceAccountFilePath: settings.googleServiceAccountFilePath
                ),
                transport: transport,
                streamingTransport: googleStreamingTransport
            )

        case .automatic:
            throw ProviderRoutingError.noPermittedProvider
        }
    }

    func makeFallbackProvider(
        settings: AppSettings,
        excluding provider: TranscriptionProviderKind
    ) throws -> (any TranscriptionProvider)? {
        switch provider {
        case .apple:
            guard settings.cloudRecognitionAllowed,
                  !settings.googleCloudProjectID.isEmpty
            else {
                return nil
            }
            return try? makeProvider(settings: settings, preference: .googleCloud)

        case .googleCloud:
            return try? makeProvider(settings: settings, preference: .apple)

        case .automatic:
            return nil
        }
    }

    private static func systemAppleOnDeviceLanguages() -> Set<String> {
        Set(SFSpeechRecognizer.supportedLocales().compactMap { locale in
            guard SFSpeechRecognizer(locale: locale)?.supportsOnDeviceRecognition == true else {
                return nil
            }
            return locale.identifier.replacingOccurrences(of: "_", with: "-")
        })
    }

    private static func systemGoogleStreamingTransport() -> (
        any GoogleStreamingRecognitionTransport
    )? {
        if #available(macOS 15.0, *) {
            return SystemGoogleStreamingRecognitionTransport()
        }
        return nil
    }
}
