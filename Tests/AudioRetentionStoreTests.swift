import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Audio retention store")
struct AudioRetentionStoreTests {
    @Test("Retained PCM survives a store reload and can be deleted")
    func retainedAudioRoundTrip() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("audio-retention-\(UUID().uuidString)")
        let store = AudioRetentionStore(directoryURL: directory)
        let entryID = UUID()
        let audio = Data([0x01, 0x02, 0x03, 0x04])

        let filename = try store.save(audio, for: entryID)

        #expect(filename == "\(entryID.uuidString).pcm")
        #expect(try AudioRetentionStore(directoryURL: directory).load(filename: filename) == audio)

        try store.delete(filename: filename)
        #expect(store.exists(filename: filename) == false)
    }

    @Test("Retention policy keeps failed audio by default")
    func failureOnlyPolicy() {
        #expect(AudioRetentionPolicy.failures.shouldRetain(deliverySucceeded: false))
        #expect(AudioRetentionPolicy.failures.shouldRetain(deliverySucceeded: true) == false)
        #expect(AudioRetentionPolicy.always.shouldRetain(deliverySucceeded: true))
        #expect(AudioRetentionPolicy.never.shouldRetain(deliverySucceeded: false) == false)
    }
}
