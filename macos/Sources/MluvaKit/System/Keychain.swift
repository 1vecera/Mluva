import Foundation
import Security

/// The ElevenLabs key lives in the login Keychain, never in settings, History or logs.
public struct KeychainItem: Sendable {
    public let service: String
    public let account: String

    public static let elevenLabs = KeychainItem(service: "com.mluva.Mac.elevenlabs", account: "api-key")

    public init(service: String, account: String) {
        self.service = service
        self.account = account
    }

    private var query: [String: Any] {
        [kSecClass as String: kSecClassGenericPassword,
         kSecAttrService as String: service,
         kSecAttrAccount as String: account]
    }

    public func read() -> String? {
        var query = query
        query[kSecReturnData as String] = true
        query[kSecMatchLimit as String] = kSecMatchLimitOne
        var result: AnyObject?
        guard SecItemCopyMatching(query as CFDictionary, &result) == errSecSuccess,
              let data = result as? Data else { return nil }
        return String(data: data, encoding: .utf8)
    }

    public func save(_ value: String) throws {
        let data = Data(value.utf8)
        let update = SecItemUpdate(query as CFDictionary, [kSecValueData as String: data] as CFDictionary)
        if update == errSecSuccess { return }
        guard update == errSecItemNotFound else { throw KeychainError(status: update) }
        var item = query
        item[kSecValueData as String] = data
        item[kSecAttrAccessible as String] = kSecAttrAccessibleAfterFirstUnlockThisDeviceOnly
        item[kSecAttrLabel as String] = "Mluva ElevenLabs API key"
        let status = SecItemAdd(item as CFDictionary, nil)
        guard status == errSecSuccess else { throw KeychainError(status: status) }
    }

    public func delete() {
        SecItemDelete(query as CFDictionary)
    }
}

public struct KeychainError: Error, LocalizedError {
    public let status: OSStatus
    public var errorDescription: String? {
        "The Keychain refused the key (\(SecCopyErrorMessageString(status, nil) as String? ?? "status \(status)"))."
    }
}

/// The speech key: the Keychain first, then `ELEVENLABS_API_KEY` from a managed launcher.
public func speechKey(
    keychain: KeychainItem = .elevenLabs,
    environment: [String: String] = ProcessInfo.processInfo.environment
) -> String? {
    if let stored = keychain.read(), !stored.isEmpty { return stored }
    return environment["ELEVENLABS_API_KEY"].flatMap { $0.isEmpty ? nil : $0 }
}
