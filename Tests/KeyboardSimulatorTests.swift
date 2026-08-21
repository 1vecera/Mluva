import Testing
import CoreGraphics
import AppKit
import Foundation
@testable import VoiceScribeMac

@Suite("Keyboard Simulator")
struct KeyboardSimulatorTests {

    @Test("Virtual key code 0x09 is the V key (for Cmd+V paste)")
    func vKeyCode() {
        // macOS virtual key codes: kVK_ANSI_V = 0x09
        // This is critical — wrong key code would paste nothing or trigger wrong shortcut
        let vKeyCode: UInt16 = 0x09

        // Create a CGEvent and verify the key code roundtrips
        guard let event = CGEvent(keyboardEventSource: nil, virtualKey: vKeyCode, keyDown: true) else {
            Issue.record("Could not create CGEvent")
            return
        }
        let readBack = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        #expect(readBack == 0x09)
    }

    @Test("R key code is 15 (0x0F) — matches GlobalHotkeyManager")
    func rKeyCode() {
        let rKeyCode: UInt16 = 15 // kVK_ANSI_R = 0x0F = 15
        guard let event = CGEvent(keyboardEventSource: nil, virtualKey: rKeyCode, keyDown: true) else {
            Issue.record("Could not create CGEvent")
            return
        }
        let readBack = UInt16(event.getIntegerValueField(.keyboardEventKeycode))
        #expect(readBack == 15)
    }

    @Test("CGEventSource with hidSystemState can be created")
    func eventSourceCreation() {
        let source = CGEventSource(stateID: .hidSystemState)
        #expect(source != nil)
    }

    // MARK: - canPaste:false (clipboard-only mode)

    @Test("typeText with canPaste:false completes without crash")
    func clipboardOnlyMode() async {
        let simulator = KeyboardSimulator()
        let testText = "clipboard-only-test-\(UUID().uuidString)"

        // Verify the clipboard-only code path completes without crash.
        // We don't assert clipboard contents because NSPasteboard.general is
        // a shared global resource that other parallel tests may clear.
        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            simulator.typeText(testText, canPaste: false) {
                continuation.resume()
            }
        }
    }

    @Test("typeText with canPaste:true completes without crash")
    func pasteModeSetsClipboard() async {
        let original = [PasteboardItemData(values: [
            "public.utf8-plain-text": Data("original".utf8)
        ])]
        let pasteboard = MutablePasteboard(items: original)
        let simulator = KeyboardSimulator(
            pasteboard: pasteboard,
            eventPoster: RecordingKeyboardEventPoster(),
            sleep: { _ in }
        )

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            simulator.typeText("paste-mode-test", canPaste: true) {
                continuation.resume()
            }
        }

        #expect(pasteboard.items == original)
    }

    @Test("Rejected clipboard write falls back to Unicode typing")
    func rejectedClipboardWriteTypesUnicode() async {
        let pasteboard = RejectingPasteboard()
        let eventPoster = RecordingKeyboardEventPoster()
        let simulator = KeyboardSimulator(
            pasteboard: pasteboard,
            eventPoster: eventPoster
        )

        await withCheckedContinuation { (continuation: CheckedContinuation<Void, Never>) in
            simulator.typeText("hello 👋", canPaste: true) {
                continuation.resume()
            }
        }

        #expect(eventPoster.pastedShortcutCount == 0)
        #expect(eventPoster.typedTexts == ["hello 👋"])
    }

    @Test("Slow paste consumer keeps dictated text installed until confirmation")
    func slowConsumerConfirmsBeforeRestore() async {
        let original = [PasteboardItemData(values: [
            "public.utf8-plain-text": Data("original".utf8)
        ])]
        let pasteboard = MutablePasteboard(items: original)
        let counter = LockedCounter()
        let simulator = KeyboardSimulator(
            pasteboard: pasteboard,
            eventPoster: RecordingKeyboardEventPoster(),
            pasteConfirmationAttempts: 5,
            pasteConfirmationDelay: 0,
            sleep: { _ in }
        )

        let outcome = await insertionOutcome(simulator, text: "dictated") {
            counter.increment() >= 3
        }

        #expect(outcome == .insertedConfirmed)
        #expect(counter.value == 3)
        #expect(pasteboard.items == original)
    }

    @Test("Unconfirmed paste leaves the complete transcript on the clipboard")
    func unconfirmedPasteKeepsRecoveryText() async {
        let pasteboard = MutablePasteboard(items: [])
        let simulator = KeyboardSimulator(
            pasteboard: pasteboard,
            eventPoster: RecordingKeyboardEventPoster(),
            pasteConfirmationAttempts: 2,
            pasteConfirmationDelay: 0,
            sleep: { _ in }
        )

        let outcome = await insertionOutcome(simulator, text: "dictated") { false }

        #expect(outcome == .pasteDispatchedWithRecovery)
        #expect(pasteboard.items == [
            PasteboardItemData(values: [
                "public.utf8-plain-text": Data("dictated".utf8)
            ])
        ])
    }

    @Test("User clipboard mutation is never overwritten after an unconfirmed paste")
    func userClipboardMutationWins() async {
        let pasteboard = MutablePasteboard(items: [])
        let simulator = KeyboardSimulator(
            pasteboard: pasteboard,
            eventPoster: RecordingKeyboardEventPoster(),
            pasteConfirmationAttempts: 1,
            pasteConfirmationDelay: 0,
            sleep: { _ in }
        )

        let outcome = await insertionOutcome(simulator, text: "dictated") {
            _ = pasteboard.replaceItems([
                PasteboardItemData(values: [
                    "public.utf8-plain-text": Data("user copy".utf8)
                ])
            ])
            return false
        }

        #expect(outcome == .unconfirmedAfterClipboardChanged)
        #expect(pasteboard.items == [
            PasteboardItemData(values: [
                "public.utf8-plain-text": Data("user copy".utf8)
            ])
        ])
    }

    private func insertionOutcome(
        _ simulator: KeyboardSimulator,
        text: String,
        confirmation: @escaping () -> Bool
    ) async -> TextInsertionOutcome {
        await withCheckedContinuation { continuation in
            simulator.insertText(
                text,
                canPaste: true,
                pasteConfirmation: confirmation
            ) { outcome in
                continuation.resume(returning: outcome)
            }
        }
    }
}

private final class RejectingPasteboard: PasteboardAccess {
    var items: [PasteboardItemData] = []
    var changeCount = 0

    func replaceItems(_ items: [PasteboardItemData]) -> Bool {
        false
    }
}

private final class RecordingKeyboardEventPoster: KeyboardEventPosting {
    private(set) var pastedShortcutCount = 0
    private(set) var typedTexts: [String] = []

    func postPasteShortcut() {
        pastedShortcutCount += 1
    }

    func postDelete() {}

    func postUnicodeText(_ text: String) {
        typedTexts.append(text)
    }
}

private final class MutablePasteboard: PasteboardAccess, @unchecked Sendable {
    private let lock = NSLock()
    private var storedItems: [PasteboardItemData]
    private var storedChangeCount = 0

    init(items: [PasteboardItemData]) {
        storedItems = items
    }

    var items: [PasteboardItemData] {
        lock.withLock { storedItems }
    }

    var changeCount: Int {
        lock.withLock { storedChangeCount }
    }

    func replaceItems(_ items: [PasteboardItemData]) -> Bool {
        lock.withLock {
            storedItems = items
            storedChangeCount += 1
        }
        return true
    }
}

private final class LockedCounter: @unchecked Sendable {
    private let lock = NSLock()
    private var count = 0

    var value: Int { lock.withLock { count } }

    func increment() -> Int {
        lock.withLock {
            count += 1
            return count
        }
    }
}
