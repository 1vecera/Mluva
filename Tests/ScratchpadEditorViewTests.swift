import AppKit
import SwiftUI
import Testing
@testable import VoiceScribeMac

@Suite("Scratchpad editor view")
struct ScratchpadEditorViewTests {
    @MainActor
    @Test("Editor renders its recovery and delivery controls")
    func rendersScratchpadEditor() throws {
        let draft = ScratchpadDraft(
            entry: TranscriptionEntry(
                rawText: "ship the native scratch pad after the tests pass",
                deliveredText: "Ship the native scratchpad after the tests pass.",
                provider: .apple,
                language: "en",
                mode: .scratchpad,
                targetApplicationName: "Notes",
                targetBundleIdentifier: "com.apple.Notes",
                deliveryOutcome: .pendingDelivery,
                retainedAudioFilename: "scratchpad.pcm"
            ),
            text: "Ship the native scratchpad after the tests pass.",
            selectedStyleID: SavedStyle.builtIns.first?.id,
            appliedStyleName: "Message",
            styleOutcome: .applied
        )
        let view = ScratchpadEditorView(
            draft: draft,
            text: .constant(draft.text),
            selectedStyleID: .constant(draft.selectedStyleID),
            styles: SavedStyle.builtIns,
            canInsert: true,
            isDelivering: false,
            isStyleWorking: false,
            onApplyStyle: {},
            onDelete: {},
            onCopy: {},
            onInsert: {}
        )
        .frame(width: 326)
        .padding(16)
        .background(Color(nsColor: .windowBackgroundColor))

        let hostingView = NSHostingView(rootView: view)
        hostingView.frame = NSRect(origin: .zero, size: hostingView.fittingSize)
        hostingView.layoutSubtreeIfNeeded()
        let bitmap = try #require(
            hostingView.bitmapImageRepForCachingDisplay(in: hostingView.bounds)
        )
        hostingView.cacheDisplay(in: hostingView.bounds, to: bitmap)

        #expect(bitmap.pixelsWide >= 326)
        #expect(bitmap.pixelsHigh >= 250)

        if let previewPath = ProcessInfo.processInfo.environment[
            "VOICE_SCRIBE_SCRATCHPAD_PREVIEW"
        ] {
            let png = try #require(bitmap.representation(using: .png, properties: [:]))
            try png.write(to: URL(fileURLWithPath: previewPath), options: .atomic)
        }
    }
}
