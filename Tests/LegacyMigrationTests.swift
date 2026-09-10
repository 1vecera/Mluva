import Foundation
import Testing
@testable import MluvaMac

@Suite("Product identity migration")
struct LegacyMigrationTests {
    @Test("State, provider preferences and drafts move once with a backup")
    func upgradesExistingState() throws {
        let support = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let old = support.appendingPathComponent("VoiceScribe")
        let current = support.appendingPathComponent("Mluva")
        let suite = "MluvaMigrationTests.\(UUID().uuidString)"
        let legacyDomain = "MluvaMigrationTests.legacy.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            try? FileManager.default.removeItem(at: support)
            defaults.removePersistentDomain(forName: suite)
            defaults.removePersistentDomain(forName: legacyDomain)
        }
        try FileManager.default.createDirectory(at: old, withIntermediateDirectories: true)
        let fixtures = [
            "transcriptions.json": "[{\"rawText\":\"VoiceScribe is historical dictated text\"}]",
            "scratchpad-draft.json": "{\"text\":\"Unfinished draft — žluťoučký\"}",
            "personalization.json": "{\"customStyles\":[]}",
            "RetainedAudio/audio.pcm": "retained audio",
            "Meetings/recordings/meeting.wav": "meeting audio",
        ]
        for (name, content) in fixtures {
            let path = old.appendingPathComponent(name)
            try FileManager.default.createDirectory(at: path.deletingLastPathComponent(), withIntermediateDirectories: true)
            try Data(content.utf8).write(to: path)
        }
        defaults.setPersistentDomain([
            "voiceScribe.language": "cs-CZ",
            "voiceScribe.googleCloudProjectID": "test-project",
            "voiceScribe.googleServiceAccountFilePath": old.appendingPathComponent("account.json").path,
            "voiceScribe.hasCompletedSetup": true,
            "voiceScribe.hotkeyKeyCode": 42,
        ], forName: legacyDomain)
        try LegacyMigration.run(support: support, defaults: defaults, domains: [legacyDomain])
        #expect(!FileManager.default.fileExists(atPath: old.path))
        for (name, content) in fixtures {
            #expect(try Data(contentsOf: current.appendingPathComponent(name)) == Data(content.utf8))
        }
        #expect(defaults.string(forKey: "mluva.language") == "cs-CZ")
        #expect(defaults.string(forKey: "mluva.googleCloudProjectID") == "test-project")
        #expect(defaults.string(forKey: "mluva.googleServiceAccountFilePath") == current.appendingPathComponent("account.json").path)
        #expect(defaults.bool(forKey: "mluva.hasCompletedSetup"))
        #expect(defaults.integer(forKey: "mluva.hotkeyKeyCode") == 42)
        #expect(defaults.persistentDomain(forName: legacyDomain) == nil)
        let backups = try FileManager.default.contentsOfDirectory(
            at: support.appendingPathComponent("Mluva-migration-backups"), includingPropertiesForKeys: nil
        )
        #expect(backups.count == 1)
        let backup = try #require(backups.first)
        #expect(try Data(contentsOf: backup.appendingPathComponent("data/scratchpad-draft.json")) == Data(fixtures["scratchpad-draft.json"]!.utf8))
        #expect(FileManager.default.fileExists(atPath: backup.appendingPathComponent("preferences.plist").path))
        #expect(FileManager.default.fileExists(atPath: backup.appendingPathComponent("complete").path))
        defaults.set("edited-project", forKey: "mluva.googleCloudProjectID")
        try LegacyMigration.run(support: support, defaults: defaults, domains: [legacyDomain])
        #expect(defaults.string(forKey: "mluva.googleCloudProjectID") == "edited-project")
        #expect(try FileManager.default.contentsOfDirectory(atPath: support.appendingPathComponent("Mluva-migration-backups").path).count == 1)
    }

    @Test("Two populated state roots fail before either is changed")
    func rejectsConflictingState() throws {
        let support = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "MluvaMigrationTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            try? FileManager.default.removeItem(at: support)
            defaults.removePersistentDomain(forName: suite)
        }
        for name in ["VoiceScribe", "Mluva"] {
            try FileManager.default.createDirectory(at: support.appendingPathComponent(name), withIntermediateDirectories: true)
        }
        #expect(throws: LegacyMigration.Conflict.self) {
            try LegacyMigration.run(support: support, defaults: defaults, domains: [])
        }
        #expect(!FileManager.default.fileExists(atPath: support.appendingPathComponent("Mluva-migration-backups").path))
    }

    @Test("Preferences already in the current domain keep their newer values")
    func preservesCurrentPreferences() throws {
        let support = FileManager.default.temporaryDirectory.appendingPathComponent(UUID().uuidString)
        let suite = "MluvaMigrationTests.\(UUID().uuidString)"
        let defaults = try #require(UserDefaults(suiteName: suite))
        defer {
            try? FileManager.default.removeItem(at: support)
            defaults.removePersistentDomain(forName: suite)
        }
        defaults.set("old-project", forKey: "voiceScribe.googleCloudProjectID")
        defaults.set("new-project", forKey: "mluva.googleCloudProjectID")
        try LegacyMigration.run(support: support, defaults: defaults, domains: [])
        #expect(defaults.string(forKey: "mluva.googleCloudProjectID") == "new-project")
        #expect(defaults.object(forKey: "voiceScribe.googleCloudProjectID") == nil)
    }
}
