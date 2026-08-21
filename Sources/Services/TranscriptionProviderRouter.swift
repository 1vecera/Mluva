import Foundation

struct TranscriptionProviderRouter {
    func resolve(
        _ request: ProviderRoutingRequest,
        capabilities: ProviderCapabilities
    ) throws -> TranscriptionProviderKind {
        switch request.preference {
        case .apple:
            guard capabilities.appleRecognitionAuthorized else {
                throw ProviderRoutingError.applePermissionRequired
            }
            guard capabilities.appleSupports(request.language) else {
                throw ProviderRoutingError.appleLanguageUnavailable(request.language)
            }
            return .apple

        case .googleCloud:
            guard capabilities.cloudAllowed else {
                throw ProviderRoutingError.cloudNotPermitted
            }
            guard capabilities.googleCloudConfigured else {
                throw ProviderRoutingError.googleCloudNotConfigured
            }
            return .googleCloud

        case .automatic:
            let appleAvailable = capabilities.appleRecognitionAuthorized
                && capabilities.appleSupports(request.language)
            let googleAvailable = capabilities.cloudAllowed && capabilities.googleCloudConfigured

            if request.needsCloudAccuracy, googleAvailable {
                return .googleCloud
            }
            if appleAvailable {
                return .apple
            }
            if googleAvailable {
                return .googleCloud
            }
            throw ProviderRoutingError.noPermittedProvider
        }
    }
}
