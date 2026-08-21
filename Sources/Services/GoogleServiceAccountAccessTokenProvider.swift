import CryptoExtras
import Foundation

protocol GoogleJWTSigning: Sendable {
    func sign(_ message: Data, privateKeyPEM: String) throws -> Data
}

struct CryptoExtrasGoogleJWTSigner: GoogleJWTSigning {
    func sign(_ message: Data, privateKeyPEM: String) throws -> Data {
        guard let privateKey = try? _RSA.Signing.PrivateKey(
            pemRepresentation: privateKeyPEM
        ), let signature = try? privateKey.signature(
            for: message,
            padding: .insecurePKCS1v1_5
        ) else {
            throw GoogleCloudSpeechError.invalidServiceAccountFile
        }
        return signature.rawRepresentation
    }
}

struct GoogleServiceAccountCredential: Decodable, Sendable {
    let type: String
    let projectID: String
    let privateKeyID: String
    let privateKey: String
    let clientEmail: String
    let tokenURI: String

    private enum CodingKeys: String, CodingKey {
        case type
        case projectID = "project_id"
        case privateKeyID = "private_key_id"
        case privateKey = "private_key"
        case clientEmail = "client_email"
        case tokenURI = "token_uri"
    }

    static func load(from fileURL: URL) throws -> Self {
        guard fileURL.isFileURL,
              let data = try? Data(contentsOf: fileURL),
              let credential = try? JSONDecoder().decode(Self.self, from: data),
              credential.isValid
        else {
            throw GoogleCloudSpeechError.invalidServiceAccountFile
        }
        return credential
    }

    private var isValid: Bool {
        guard type == "service_account",
              !projectID.isEmpty,
              !privateKeyID.isEmpty,
              privateKey.contains("-----BEGIN PRIVATE KEY-----"),
              !clientEmail.isEmpty,
              let url = URL(string: tokenURI),
              url.scheme == "https",
              ["oauth2.googleapis.com", "accounts.google.com"].contains(url.host)
        else {
            return false
        }
        return true
    }
}

final class GoogleServiceAccountAccessTokenProvider: GoogleAccessTokenProviding, @unchecked Sendable {
    private struct TokenResponse: Decodable {
        let accessToken: String
        let expiresIn: TimeInterval

        private enum CodingKeys: String, CodingKey {
            case accessToken = "access_token"
            case expiresIn = "expires_in"
        }
    }

    private struct CachedToken {
        let value: String
        let expiresAt: Date
    }

    private let fileURL: URL
    private let signer: any GoogleJWTSigning
    private let transport: any HTTPDataTransport
    private let now: @Sendable () -> Date
    private let lock = NSLock()
    private var cachedToken: CachedToken?

    init(
        fileURL: URL,
        signer: any GoogleJWTSigning = CryptoExtrasGoogleJWTSigner(),
        transport: any HTTPDataTransport = URLSessionHTTPDataTransport(),
        now: @escaping @Sendable () -> Date = { Date() }
    ) {
        self.fileURL = fileURL
        self.signer = signer
        self.transport = transport
        self.now = now
    }

    func accessToken() async throws -> String {
        let requestTime = now()
        if let token = lock.withLock({ cachedToken }),
           token.expiresAt.timeIntervalSince(requestTime) > 60 {
            return token.value
        }

        let credential = try GoogleServiceAccountCredential.load(from: fileURL)
        let assertion = try makeAssertion(credential: credential, issuedAt: requestTime)
        guard let tokenURL = URL(string: credential.tokenURI) else {
            throw GoogleCloudSpeechError.invalidServiceAccountFile
        }
        var request = URLRequest(url: tokenURL)
        request.httpMethod = "POST"
        request.setValue(
            "application/x-www-form-urlencoded",
            forHTTPHeaderField: "Content-Type"
        )
        var body = URLComponents()
        body.queryItems = [
            URLQueryItem(
                name: "grant_type",
                value: "urn:ietf:params:oauth:grant-type:jwt-bearer"
            ),
            URLQueryItem(name: "assertion", value: assertion),
        ]
        request.httpBody = body.percentEncodedQuery?.data(using: .utf8)

        let (data, response) = try await transport.data(for: request)
        guard (200..<300).contains(response.statusCode),
              let token = try? JSONDecoder().decode(TokenResponse.self, from: data),
              !token.accessToken.isEmpty,
              token.expiresIn > 0
        else {
            throw GoogleCloudSpeechError.authenticationUnavailable
        }
        lock.withLock {
            cachedToken = CachedToken(
                value: token.accessToken,
                expiresAt: requestTime.addingTimeInterval(token.expiresIn)
            )
        }
        return token.accessToken
    }

    private func makeAssertion(
        credential: GoogleServiceAccountCredential,
        issuedAt: Date
    ) throws -> String {
        let issuedAtSeconds = Int(issuedAt.timeIntervalSince1970)
        let header: [String: Any] = [
            "alg": "RS256",
            "kid": credential.privateKeyID,
            "typ": "JWT",
        ]
        let claims: [String: Any] = [
            "aud": credential.tokenURI,
            "exp": issuedAtSeconds + 3_600,
            "iat": issuedAtSeconds,
            "iss": credential.clientEmail,
            "scope": "https://www.googleapis.com/auth/cloud-platform",
        ]
        let encodedHeader = try encodedJSON(header)
        let encodedClaims = try encodedJSON(claims)
        let unsignedAssertion = "\(encodedHeader).\(encodedClaims)"
        guard let assertionData = unsignedAssertion.data(using: .utf8) else {
            throw GoogleCloudSpeechError.invalidServiceAccountFile
        }
        let signature = try signer.sign(
            assertionData,
            privateKeyPEM: credential.privateKey
        )
        return "\(unsignedAssertion).\(base64URLEncoded(signature))"
    }

    private func encodedJSON(_ object: [String: Any]) throws -> String {
        guard JSONSerialization.isValidJSONObject(object),
              let data = try? JSONSerialization.data(
                withJSONObject: object,
                options: [.sortedKeys]
              )
        else {
            throw GoogleCloudSpeechError.invalidServiceAccountFile
        }
        return base64URLEncoded(data)
    }

    private func base64URLEncoded(_ data: Data) -> String {
        data.base64EncodedString()
            .replacingOccurrences(of: "+", with: "-")
            .replacingOccurrences(of: "/", with: "_")
            .replacingOccurrences(of: "=", with: "")
    }
}

struct GoogleAccessTokenProviderFactory: Sendable {
    func makeProvider(serviceAccountFilePath: String) -> any GoogleAccessTokenProviding {
        let path = serviceAccountFilePath.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !path.isEmpty else {
            return GCloudApplicationDefaultCredentialsTokenProvider()
        }
        return GoogleServiceAccountAccessTokenProvider(
            fileURL: URL(fileURLWithPath: path)
        )
    }
}
