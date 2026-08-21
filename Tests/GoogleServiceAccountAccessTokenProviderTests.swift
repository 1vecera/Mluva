import Foundation
import Security
import Testing
@testable import VoiceScribeMac

@Suite("Google service-account access token")
struct GoogleServiceAccountAccessTokenProviderTests {
    @Test("Selected credential signs a short-lived cloud-platform assertion")
    func exchangesSignedAssertion() async throws {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-service-account-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: fileURL) }
        // Keep the synthetic PEM markers split so scanners cannot mistake this fixture for a credential.
        let fakePrivateKey = [
            "-----BEGIN",
            " PRIVATE KEY-----\\n",
            "private-key-data\\n",
            "-----END",
            " PRIVATE KEY-----\\n",
        ].joined()
        let credential = """
        {
          "type": "service_account",
          "project_id": "example-project",
          "private_key_id": "key-id",
          "private_key": "\(fakePrivateKey)",
          "client_email": "voice@example-project.iam.gserviceaccount.com",
          "token_uri": "https://oauth2.googleapis.com/token"
        }
        """
        try Data(credential.utf8).write(to: fileURL)
        let signer = RecordingGoogleJWTSigner(signature: Data([0xFA, 0xCE]))
        let transport = RecordingTokenExchangeTransport()
        let provider = GoogleServiceAccountAccessTokenProvider(
            fileURL: fileURL,
            signer: signer,
            transport: transport,
            now: { Date(timeIntervalSince1970: 1_000) }
        )

        let token = try await provider.accessToken()
        let cachedToken = try await provider.accessToken()

        #expect(token == "service-account-token")
        #expect(cachedToken == token)
        #expect(transport.requestCount == 1)
        let request = try #require(transport.request)
        #expect(request.url?.absoluteString == "https://oauth2.googleapis.com/token")
        #expect(request.httpMethod == "POST")
        #expect(request.value(forHTTPHeaderField: "Content-Type") == "application/x-www-form-urlencoded")
        let body = String(data: try #require(request.httpBody), encoding: .utf8) ?? ""
        #expect(!body.contains("private-key-data"))
        let items = URLComponents(string: "?\(body)")?.queryItems ?? []
        #expect(items.first(where: { $0.name == "grant_type" })?.value
            == "urn:ietf:params:oauth:grant-type:jwt-bearer")
        let assertion = try #require(items.first(where: { $0.name == "assertion" })?.value)
        let segments = assertion.split(separator: ".").map(String.init)
        #expect(segments.count == 3)
        let header = try decodedJSONObject(segments[0])
        let claims = try decodedJSONObject(segments[1])
        #expect(header["alg"] as? String == "RS256")
        #expect(header["typ"] as? String == "JWT")
        #expect(header["kid"] as? String == "key-id")
        #expect(claims["iss"] as? String == "voice@example-project.iam.gserviceaccount.com")
        #expect(claims["scope"] as? String == "https://www.googleapis.com/auth/cloud-platform")
        #expect(claims["aud"] as? String == "https://oauth2.googleapis.com/token")
        #expect(claims["iat"] as? Int == 1_000)
        #expect(claims["exp"] as? Int == 4_600)
        #expect(signer.privateKey?.contains("private-key-data") == true)
        #expect(signer.message.flatMap { String(data: $0, encoding: .utf8) }
            == segments.dropLast().joined(separator: "."))
    }

    @Test("Malformed selected credential fails without a network request")
    func rejectsMalformedCredential() async throws {
        let fileURL = FileManager.default.temporaryDirectory
            .appendingPathComponent("mluva-invalid-service-account-\(UUID().uuidString).json")
        defer { try? FileManager.default.removeItem(at: fileURL) }
        try Data("{}".utf8).write(to: fileURL)
        let transport = RecordingTokenExchangeTransport()
        let provider = GoogleServiceAccountAccessTokenProvider(
            fileURL: fileURL,
            signer: RecordingGoogleJWTSigner(signature: Data()),
            transport: transport
        )

        await #expect(throws: GoogleCloudSpeechError.invalidServiceAccountFile) {
            try await provider.accessToken()
        }
        #expect(transport.request == nil)
    }

    @Test("Credential selection chooses service account or ADC without reading a key")
    func selectsCredentialProvider() {
        let factory = GoogleAccessTokenProviderFactory()

        #expect(factory.makeProvider(serviceAccountFilePath: "")
            is GCloudApplicationDefaultCredentialsTokenProvider)
        #expect(factory.makeProvider(serviceAccountFilePath: "/private/account.json")
            is GoogleServiceAccountAccessTokenProvider)
    }

    @Test("RSA signer imports the PKCS8 key format emitted by Google")
    func signsGooglePKCS8Key() throws {
        let attributes: [CFString: Any] = [
            kSecAttrKeyType: kSecAttrKeyTypeRSA,
            kSecAttrKeySizeInBits: 2_048,
        ]
        var generationError: Unmanaged<CFError>?
        let privateKey = try #require(SecKeyCreateRandomKey(
            attributes as CFDictionary,
            &generationError
        ))
        var exportError: Unmanaged<CFError>?
        let pkcs1 = try #require(
            SecKeyCopyExternalRepresentation(privateKey, &exportError) as Data?
        )
        let pkcs8 = derSequence(
            Data([0x02, 0x01, 0x00])
                + Data([
                    0x30, 0x0D,
                    0x06, 0x09, 0x2A, 0x86, 0x48, 0x86, 0xF7, 0x0D, 0x01, 0x01, 0x01,
                    0x05, 0x00,
                ])
                + derValue(tag: 0x04, value: pkcs1)
        )
        let pemLabel = "PRIVATE KEY"
        let pem = """
        -----BEGIN \(pemLabel)-----
        \(pkcs8.base64EncodedString())
        -----END \(pemLabel)-----
        """
        let message = Data("header.claims".utf8)

        let signature = try CryptoExtrasGoogleJWTSigner().sign(
            message,
            privateKeyPEM: pem
        )

        let publicKey = try #require(SecKeyCopyPublicKey(privateKey))
        var verificationError: Unmanaged<CFError>?
        #expect(SecKeyVerifySignature(
            publicKey,
            .rsaSignatureMessagePKCS1v15SHA256,
            message as CFData,
            signature as CFData,
            &verificationError
        ))
    }

    private func decodedJSONObject(_ segment: String) throws -> [String: Any] {
        var value = segment.replacingOccurrences(of: "-", with: "+")
            .replacingOccurrences(of: "_", with: "/")
        value += String(repeating: "=", count: (4 - value.count % 4) % 4)
        let data = try #require(Data(base64Encoded: value))
        return try #require(JSONSerialization.jsonObject(with: data) as? [String: Any])
    }

    private func derSequence(_ value: Data) -> Data {
        derValue(tag: 0x30, value: value)
    }

    private func derValue(tag: UInt8, value: Data) -> Data {
        Data([tag]) + derLength(value.count) + value
    }

    private func derLength(_ length: Int) -> Data {
        guard length >= 0x80 else { return Data([UInt8(length)]) }
        var remaining = length
        var bytes: [UInt8] = []
        while remaining > 0 {
            bytes.insert(UInt8(remaining & 0xFF), at: 0)
            remaining >>= 8
        }
        return Data([0x80 | UInt8(bytes.count)] + bytes)
    }
}

private final class RecordingGoogleJWTSigner: GoogleJWTSigning, @unchecked Sendable {
    private let signature: Data
    private(set) var message: Data?
    private(set) var privateKey: String?

    init(signature: Data) {
        self.signature = signature
    }

    func sign(_ message: Data, privateKeyPEM: String) throws -> Data {
        self.message = message
        privateKey = privateKeyPEM
        return signature
    }
}

private final class RecordingTokenExchangeTransport: HTTPDataTransport, @unchecked Sendable {
    private(set) var request: URLRequest?
    private(set) var requestCount = 0

    func data(for request: URLRequest) async throws -> (Data, HTTPURLResponse) {
        self.request = request
        requestCount += 1
        guard let url = request.url,
              let response = HTTPURLResponse(
            url: url,
            statusCode: 200,
            httpVersion: nil,
            headerFields: nil
        ) else {
            throw GoogleCloudSpeechError.invalidResponse
        }
        return (Data(#"{"access_token":"service-account-token","expires_in":3600,"token_type":"Bearer"}"#.utf8), response)
    }
}
