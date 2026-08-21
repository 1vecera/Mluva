import AppKit
import CoreGraphics
import Foundation
import ApplicationServices

protocol TextInserting: AnyObject {
    func insertText(
        _ text: String,
        canPaste: Bool,
        pasteConfirmation: (() -> Bool)?,
        completion: @escaping (TextInsertionOutcome) -> Void
    )
}

enum TextInsertionOutcome: Equatable, Sendable {
    case insertedConfirmed
    case pasteDispatchedWithRecovery
    case copiedForManualPaste
    case unconfirmedAfterClipboardChanged
    case clipboardUnavailable
}

protocol KeyboardEventPosting: AnyObject {
    func postPasteShortcut()
    func postDelete()
    func postUnicodeText(_ text: String)
}

final class KeyboardSimulator {
    private let queue = DispatchQueue(label: "com.voicescribe.keyboard", qos: .userInteractive)
    private let clipboard: ClipboardTransactionController
    private let eventPoster: any KeyboardEventPosting
    private let pasteConfirmationAttempts: Int
    private let pasteConfirmationDelay: useconds_t
    private let sleep: (useconds_t) -> Void

    init(
        pasteboard: any PasteboardAccess = SystemPasteboardAccess(),
        eventPoster: any KeyboardEventPosting = SystemKeyboardEventPoster(),
        pasteConfirmationAttempts: Int = 40,
        pasteConfirmationDelay: useconds_t = 50_000,
        sleep: @escaping (useconds_t) -> Void = { _ = usleep($0) }
    ) {
        clipboard = ClipboardTransactionController(pasteboard: pasteboard)
        self.eventPoster = eventPoster
        self.pasteConfirmationAttempts = max(1, pasteConfirmationAttempts)
        self.pasteConfirmationDelay = pasteConfirmationDelay
        self.sleep = sleep
    }

    /// Paste text into the currently focused application via clipboard + Cmd+V.
    /// Serialized on a dedicated queue to prevent overlapping pastes.
    /// When `canPaste` is false (no Accessibility), copies text to clipboard only.
    func typeText(_ text: String, canPaste: Bool = true, completion: (() -> Void)? = nil) {
        queue.async { [weak self] in
            if canPaste {
                self?.pasteWithoutConfirmation(text)
            } else {
                _ = self?.copyToClipboardOnly(text)
            }
            DispatchQueue.main.async { completion?() }
        }
    }

    func insertText(
        _ text: String,
        canPaste: Bool,
        pasteConfirmation: (() -> Bool)?,
        completion: @escaping (TextInsertionOutcome) -> Void
    ) {
        queue.async { [weak self] in
            let outcome: TextInsertionOutcome
            if let self, canPaste {
                outcome = self.pasteViaClipboard(
                    text,
                    pasteConfirmation: pasteConfirmation
                )
            } else if let self {
                outcome = self.copyToClipboardOnly(text)
            } else {
                outcome = .clipboardUnavailable
            }
            DispatchQueue.main.async { completion(outcome) }
        }
    }

    func replaceTypedSnippet(_ replacement: TypedSnippetReplacement) {
        queue.async { [weak self] in
            self?.deleteCharacters(replacement.characterCount)
            self?.pasteWithoutConfirmation(replacement.text + replacement.trailingText)
        }
    }

    private func deleteCharacters(_ count: Int) {
        guard count > 0 else { return }
        for _ in 0..<count {
            eventPoster.postDelete()
        }
        usleep(15_000)
    }

    /// Full paste: save clipboard, write text, simulate Cmd+V, restore clipboard
    private func pasteViaClipboard(
        _ text: String,
        pasteConfirmation: (() -> Bool)?
    ) -> TextInsertionOutcome {
        guard let transaction = clipboard.install(text: text) else {
            eventPoster.postUnicodeText(text)
            if waitForPasteConfirmation(pasteConfirmation) {
                return .insertedConfirmed
            }
            return copyToClipboardOnly(text)
        }

        // Brief pause for clipboard to settle
        sleep(15_000)

        eventPoster.postPasteShortcut()

        guard waitForPasteConfirmation(pasteConfirmation) else {
            return clipboard.ownsInstalledText(transaction)
                ? .pasteDispatchedWithRecovery
                : .unconfirmedAfterClipboardChanged
        }

        _ = clipboard.restore(transaction)
        return .insertedConfirmed
    }

    private func pasteWithoutConfirmation(_ text: String) {
        guard let transaction = clipboard.install(text: text) else {
            eventPoster.postUnicodeText(text)
            return
        }
        sleep(15_000)
        eventPoster.postPasteShortcut()
        sleep(150_000)
        _ = clipboard.restore(transaction)
    }

    /// Clipboard-only mode: just set clipboard text without simulating Cmd+V.
    /// Used when Accessibility is not granted (CGEvent.post would silently fail).
    private func copyToClipboardOnly(_ text: String) -> TextInsertionOutcome {
        clipboard.copyOnly(text: text) ? .copiedForManualPaste : .clipboardUnavailable
    }

    private func waitForPasteConfirmation(_ confirmation: (() -> Bool)?) -> Bool {
        guard let confirmation else { return false }
        for _ in 0..<pasteConfirmationAttempts {
            sleep(pasteConfirmationDelay)
            if confirmation() { return true }
        }
        return false
    }
}

extension KeyboardSimulator: TextInserting {}
extension KeyboardSimulator: TypedSnippetReplacing {}

final class SystemKeyboardEventPoster: KeyboardEventPosting {
    func postPasteShortcut() {
        let source = CGEventSource(stateID: .hidSystemState)
        let keyDown = CGEvent(
            keyboardEventSource: source,
            virtualKey: 0x09,
            keyDown: true
        )
        let keyUp = CGEvent(
            keyboardEventSource: source,
            virtualKey: 0x09,
            keyDown: false
        )
        keyDown?.flags = .maskCommand
        keyUp?.flags = .maskCommand
        keyDown?.post(tap: .cghidEventTap)
        usleep(15_000)
        keyUp?.post(tap: .cghidEventTap)
    }

    func postDelete() {
        let source = CGEventSource(stateID: .hidSystemState)
        let keyDown = CGEvent(
            keyboardEventSource: source,
            virtualKey: 51,
            keyDown: true
        )
        let keyUp = CGEvent(
            keyboardEventSource: source,
            virtualKey: 51,
            keyDown: false
        )
        keyDown?.post(tap: .cghidEventTap)
        keyUp?.post(tap: .cghidEventTap)
    }

    func postUnicodeText(_ text: String) {
        for chunk in unicodeEventChunks(text) {
            let source = CGEventSource(stateID: .hidSystemState)
            guard let keyDown = CGEvent(
                keyboardEventSource: source,
                virtualKey: 0,
                keyDown: true
            ), let keyUp = CGEvent(
                keyboardEventSource: source,
                virtualKey: 0,
                keyDown: false
            ) else {
                continue
            }
            let utf16 = Array(chunk.utf16)
            utf16.withUnsafeBufferPointer { buffer in
                keyDown.keyboardSetUnicodeString(
                    stringLength: buffer.count,
                    unicodeString: buffer.baseAddress
                )
            }
            keyDown.post(tap: .cghidEventTap)
            keyUp.post(tap: .cghidEventTap)
        }
    }

    private func unicodeEventChunks(_ text: String) -> [String] {
        var chunks: [String] = []
        var current = ""

        for character in text {
            let addition = String(character)
            if !current.isEmpty, current.utf16.count + addition.utf16.count > 20 {
                chunks.append(current)
                current = ""
            }
            current.append(character)
        }
        if !current.isEmpty {
            chunks.append(current)
        }
        return chunks
    }
}
