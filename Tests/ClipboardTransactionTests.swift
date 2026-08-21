import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Clipboard transaction")
struct ClipboardTransactionTests {
    @Test("Restore preserves every original item and pasteboard type")
    func restoresAllItemsAndTypes() throws {
        let original = [
            PasteboardItemData(values: [
                "public.utf8-plain-text": Data("plain".utf8),
                "public.html": Data("<b>rich</b>".utf8),
            ]),
            PasteboardItemData(values: [
                "public.url": Data("https://example.com".utf8),
            ]),
        ]
        let pasteboard = InMemoryPasteboard(items: original)
        let controller = ClipboardTransactionController(pasteboard: pasteboard)

        let transaction = try #require(controller.install(text: "dictated"))
        #expect(pasteboard.items == [
            PasteboardItemData(values: ["public.utf8-plain-text": Data("dictated".utf8)])
        ])

        #expect(controller.restore(transaction))
        #expect(pasteboard.items == original)
    }

    @Test("Restore does not overwrite a clipboard changed by the user")
    func preservesConcurrentClipboardChange() throws {
        let pasteboard = InMemoryPasteboard(items: [
            PasteboardItemData(values: ["public.utf8-plain-text": Data("before".utf8)])
        ])
        let controller = ClipboardTransactionController(pasteboard: pasteboard)

        let transaction = try #require(controller.install(text: "dictated"))
        pasteboard.replaceItems([
            PasteboardItemData(values: ["public.utf8-plain-text": Data("user copy".utf8)])
        ])

        #expect(!controller.restore(transaction))
        #expect(pasteboard.items == [
            PasteboardItemData(values: ["public.utf8-plain-text": Data("user copy".utf8)])
        ])
    }

    @Test("Clipboard-only delivery intentionally keeps dictated text")
    func clipboardOnlyKeepsText() {
        let pasteboard = InMemoryPasteboard(items: [])
        let controller = ClipboardTransactionController(pasteboard: pasteboard)

        controller.copyOnly(text: "dictated")

        #expect(pasteboard.items == [
            PasteboardItemData(values: ["public.utf8-plain-text": Data("dictated".utf8)])
        ])
    }
}

private final class InMemoryPasteboard: PasteboardAccess {
    private(set) var items: [PasteboardItemData]
    private(set) var changeCount = 0

    init(items: [PasteboardItemData]) {
        self.items = items
    }

    @discardableResult
    func replaceItems(_ items: [PasteboardItemData]) -> Bool {
        self.items = items
        changeCount += 1
        return true
    }
}
