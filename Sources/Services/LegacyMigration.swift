import Foundation

/// The only runtime boundary that knows the retired product's storage identities.
/// Runs before any settings or stores are constructed; failures leave the app closed.
enum LegacyMigration {
    static let legacyDomains = ["com.voicescribe.mac", "VoiceScribeMac"]
    static let legacyPrefix = "voiceScribe."

    struct Conflict: LocalizedError {
        var errorDescription: String? {
            "Both legacy and Mluva data directories exist. Reconcile them before opening Mluva; neither was changed."
        }
    }

    static func run(
        support: URL = FileManager.default.urls(for: .applicationSupportDirectory, in: .userDomainMask)[0],
        defaults: UserDefaults = .standard,
        domains: [String] = legacyDomains,
        fileManager: FileManager = .default
    ) throws {
        let old = support.appendingPathComponent("VoiceScribe", isDirectory: true)
        let current = support.appendingPathComponent("Mluva", isDirectory: true)
        let oldExists = fileManager.fileExists(atPath: old.path)
        let snapshots = domains.compactMap { domain -> (String, [String: Any])? in
            guard let values = defaults.persistentDomain(forName: domain), !values.isEmpty else { return nil }
            return (domain, values)
        }
        let localLegacy = defaults.dictionaryRepresentation().filter { $0.key.hasPrefix(legacyPrefix) }
        guard oldExists || !snapshots.isEmpty || !localLegacy.isEmpty else { return }
        if oldExists && fileManager.fileExists(atPath: current.path) { throw Conflict() }
        if oldExists {
            let values = try old.resourceValues(forKeys: [.isSymbolicLinkKey, .isDirectoryKey])
            guard values.isSymbolicLink != true, values.isDirectory == true else { throw Conflict() }
        }

        let backupRoot = support.appendingPathComponent("Mluva-migration-backups", isDirectory: true)
        if (try? backupRoot.resourceValues(forKeys: [.isSymbolicLinkKey]).isSymbolicLink) == true {
            throw Conflict()
        }
        let backup = backupRoot.appendingPathComponent(UUID().uuidString, isDirectory: true)
        try fileManager.createDirectory(at: backup, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        try fileManager.setAttributes([.posixPermissions: 0o700], ofItemAtPath: backupRoot.path)
        if oldExists { try fileManager.copyItem(at: old, to: backup.appendingPathComponent("data")) }
        let preferenceBackup: [String: Any] = [
            "domains": Dictionary(uniqueKeysWithValues: snapshots),
            "local": localLegacy,
        ]
        try PropertyListSerialization.data(fromPropertyList: preferenceBackup, format: .binary, options: 0)
            .write(to: backup.appendingPathComponent("preferences.plist"), options: .atomic)

        var importedKeys: [String] = []
        do {
            if oldExists { try fileManager.moveItem(at: old, to: current) }
            // Current preferences win if a user has already configured the new bundle.
            let legacyValues = snapshots.reduce(localLegacy) { values, domain in
                values.merging(domain.1) { current, _ in current }
            }
            for (key, value) in legacyValues where key.hasPrefix(legacyPrefix) {
                let newKey = "mluva." + key.dropFirst(legacyPrefix.count)
                guard defaults.object(forKey: newKey) == nil else { continue }
                var migratedValue = value
                if newKey == "mluva.googleServiceAccountFilePath", let path = value as? String,
                   path.hasPrefix(old.path + "/") {
                    migratedValue = current.path + path.dropFirst(old.path.count)
                }
                defaults.set(migratedValue, forKey: newKey)
                importedKeys.append(newKey)
            }
            guard defaults.synchronize() else {
                throw CocoaError(.fileWriteUnknown)
            }
            for (domain, _) in snapshots { defaults.removePersistentDomain(forName: domain) }
            for key in localLegacy.keys { defaults.removeObject(forKey: key) }
            guard defaults.synchronize() else { throw CocoaError(.fileWriteUnknown) }
            try Data().write(to: backup.appendingPathComponent("complete"), options: .atomic)
        } catch {
            for key in importedKeys { defaults.removeObject(forKey: key) }
            for (domain, values) in snapshots { defaults.setPersistentDomain(values, forName: domain) }
            for (key, value) in localLegacy { defaults.set(value, forKey: key) }
            defaults.synchronize()
            if oldExists && !fileManager.fileExists(atPath: old.path) {
                try fileManager.moveItem(at: current, to: old)
            }
            throw error
        }
    }
}
