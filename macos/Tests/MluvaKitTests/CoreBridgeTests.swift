import Foundation
import Testing
@testable import MluvaKit

/// The bridge against real stores in a private temporary directory. No microphone, provider,
/// Herdr server or host clipboard is used.
@Suite(.serialized)
struct CoreBridgeTests {
    let root: URL

    init() throws {
        root = FileManager.default.temporaryDirectory.appending(path: "mluva-tests-\(UUID().uuidString)")
        let paths = AppPaths(
            config: root.appending(path: "config"), data: root.appending(path: "data"),
            herdrConfigHome: root.appending(path: "xdg"))
        try CoreBridge.shared.call("open", [
            "config_dir": .string(paths.config.path), "data_dir": .string(paths.data.path),
            "herdr_config_home": .string(paths.herdrConfigHome.path),
            "claude_config_dir": .string(root.appending(path: "claude").path),
        ])
    }

    @Test func settingsDecodeIntoTheSharedModelAndRoundTrip() throws {
        var config = try #require(try CoreBridge.shared.call("config.load")["config"]?.decode(AppConfig.self))
        #expect(config.transcription_provider == "elevenlabs")
        #expect(config.global_recording_key == "F9")
        config.language_code = "ces"
        config.incognito_mode = true
        try CoreBridge.shared.call("config.save", ["config": try JSONValue(encoding: config)])
        let reloaded = try CoreBridge.shared.call("config.load")["config"]?.decode(AppConfig.self)
        #expect(reloaded == config)
    }

    @Test func historyKeepsTheRawRecognitionBesideEdits() throws {
        let input = HistoryInput(raw: "hello mluva", delivered: "Hello Mluva", language: "eng", outcome: "copied")
        let entry = try CoreBridge.shared.call("history.add", ["input": try JSONValue(encoding: input)], as: HistoryEntry.self)
        #expect(entry.raw_text == "hello mluva")
        #expect(entry.createdAt != nil)
        try CoreBridge.shared.call("history.save_text", ["id": .string(entry.identifier), "text": "Edited"])
        #expect(try CoreBridge.shared.call("history.source", ["id": .string(entry.identifier)]).string == "Edited")
        let found = try CoreBridge.shared.call("history.find", ["id": .string(entry.identifier)], as: HistoryEntry.self)
        #expect(found.raw_text == "hello mluva")
        #expect(found.displayTitle == "Hello Mluva")
    }

    @Test func voiceSettingsDefaultToTheLinuxKeys() throws {
        let settings = try CoreBridge.shared.call("voice.load", as: VoiceSettings.self)
        #expect(settings.hold_key == "F8")
        #expect(settings.draft_key == "F7")
        #expect(settings.enabled == false)
    }

    @Test func backgroundTasksResumeTheirCaller() async throws {
        let found = try await CoreBridge.shared.task("agents.discover", [:], as: Discovery.self)
        #expect(found.herdr == false)
        #expect(found.sessions.isEmpty)
    }

    @Test func errorsCarryTheCoreMessage() {
        #expect(throws: BridgeError.self) { try CoreBridge.shared.call("no.such.method") }
    }
}
