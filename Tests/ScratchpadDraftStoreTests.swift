import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Scratchpad draft store")
struct ScratchpadDraftStoreTests {
    @Test("An unfinished draft survives a store restart")
    func draftSurvivesRestart() throws {
        let fileURL = temporaryFileURL(named: "draft")
        let entry = TranscriptionEntry(
            rawText: "raw thinking",
            deliveredText: "Clean thinking",
            provider: .apple,
            language: "en",
            mode: .scratchpad,
            deliveryOutcome: .pendingDelivery,
            retainedAudioFilename: "capture.pcm"
        )
        let store = ScratchpadDraftStore(fileURL: fileURL)
        let styleID = try #require(SavedStyle.builtIns.first?.id)

        store.save(
            ScratchpadDraft(
                entry: entry,
                text: "Edited thinking",
                selectedStyleID: styleID,
                appliedStyleName: "Message",
                styleOutcome: .applied
            ),
            persist: true
        )
        #expect(store.persistenceError == nil)

        let restoredStore = ScratchpadDraftStore(fileURL: fileURL)
        #expect(restoredStore.persistenceError == nil)
        let restored = try #require(restoredStore.draft)
        #expect(restored.id == entry.id)
        #expect(restored.entry.rawText == "raw thinking")
        #expect(restored.text == "Edited thinking")
        #expect(restored.entry.retainedAudioFilename == "capture.pcm")
        #expect(restored.selectedStyleID == styleID)
        #expect(restored.appliedStyleName == "Message")
        #expect(restored.styleOutcome == .applied)
        let attributes = try FileManager.default.attributesOfItem(atPath: fileURL.path)
        #expect((attributes[.posixPermissions] as? NSNumber)?.intValue == 0o600)
    }

    @Test("Clearing a draft removes its durable state")
    func clearRemovesDurableState() {
        let fileURL = temporaryFileURL(named: "clear")
        let store = ScratchpadDraftStore(fileURL: fileURL)
        let entry = TranscriptionEntry(
            rawText: "raw",
            deliveredText: "draft",
            provider: .apple,
            language: "en",
            mode: .scratchpad,
            deliveryOutcome: .pendingDelivery
        )
        store.save(ScratchpadDraft(entry: entry, text: "draft"), persist: true)

        store.clear()

        #expect(store.draft == nil)
        #expect(ScratchpadDraftStore(fileURL: fileURL).draft == nil)
    }

    @Test("Incognito drafts remain memory-only")
    func incognitoDraftIsNotPersisted() {
        let fileURL = temporaryFileURL(named: "incognito")
        let store = ScratchpadDraftStore(fileURL: fileURL)
        let entry = TranscriptionEntry(
            rawText: "private",
            deliveredText: "private",
            provider: .apple,
            language: "en",
            mode: .scratchpad,
            deliveryOutcome: .pendingDelivery
        )

        store.save(ScratchpadDraft(entry: entry, text: "private"), persist: false)

        #expect(store.draft?.text == "private")
        #expect(ScratchpadDraftStore(fileURL: fileURL).draft == nil)
    }

    private func temporaryFileURL(named name: String) -> URL {
        FileManager.default.temporaryDirectory
            .appendingPathComponent(
                "scratchpad-\(name)-\(UUID().uuidString)",
                isDirectory: true
            )
            .appendingPathComponent("scratchpad-draft.json")
    }
}
