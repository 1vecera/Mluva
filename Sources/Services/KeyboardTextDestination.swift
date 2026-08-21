import Foundation

enum TextDestinationError: Error, Equatable, LocalizedError {
    case targetUnavailable
    case manualPasteRequired
    case insertionUnconfirmed
    case clipboardUnavailable

    var errorDescription: String? {
        switch self {
        case .targetUnavailable:
            return "The original field is unavailable. Your text is in history and on the clipboard; paste it manually."
        case .manualPasteRequired:
            return "Automatic insertion needs Accessibility permission. Your text is in history and on the clipboard; paste it manually."
        case .insertionUnconfirmed:
            return "Automatic insertion could not be confirmed. Your text remains recoverable in history; check the clipboard before retrying."
        case .clipboardUnavailable:
            return "Automatic insertion and clipboard recovery failed. Your complete text remains in history."
        }
    }
}

final class KeyboardTextDestination: TextDestination, @unchecked Sendable {
    private let textInserter: any TextInserting
    private let canPaste: () -> Bool
    private let target: (any TextTargetRestoring)?

    init(
        textInserter: any TextInserting,
        canPaste: @escaping () -> Bool,
        target: (any TextTargetRestoring)?
    ) {
        self.textInserter = textInserter
        self.canPaste = canPaste
        self.target = target
    }

    convenience init(
        keyboardSimulator: KeyboardSimulator,
        canPaste: @escaping () -> Bool,
        target: (any TextTargetRestoring)? = nil
    ) {
        self.init(
            textInserter: keyboardSimulator,
            canPaste: canPaste,
            target: target
        )
    }

    func insert(_ delivery: TextDelivery) async throws {
        let shouldPaste = canPaste()
        guard shouldPaste else {
            let outcome = await insert(
                delivery.text,
                canPaste: false,
                pasteConfirmation: nil
            )
            throw recoveryError(for: outcome, fallback: .manualPasteRequired)
        }
        guard let target,
              target.restore()
        else {
            let outcome = await insert(
                delivery.text,
                canPaste: false,
                pasteConfirmation: nil
            )
            throw recoveryError(for: outcome, fallback: .targetUnavailable)
        }
        if target.replaceSelection(with: delivery.text),
           target.confirmInsertion(of: delivery.text) {
            return
        }

        guard target.restore() else {
            let outcome = await insert(
                delivery.text,
                canPaste: false,
                pasteConfirmation: nil
            )
            throw recoveryError(for: outcome, fallback: .targetUnavailable)
        }
        let outcome = await insert(
            delivery.text,
            canPaste: true,
            pasteConfirmation: { target.confirmInsertion(of: delivery.text) }
        )
        switch outcome {
        case .insertedConfirmed:
            return
        case .pasteDispatchedWithRecovery:
            return
        case .copiedForManualPaste:
            throw TextDestinationError.insertionUnconfirmed
        case .unconfirmedAfterClipboardChanged:
            throw TextDestinationError.insertionUnconfirmed
        case .clipboardUnavailable:
            throw TextDestinationError.clipboardUnavailable
        }
    }

    private func insert(
        _ text: String,
        canPaste: Bool,
        pasteConfirmation: (() -> Bool)?
    ) async -> TextInsertionOutcome {
        await withCheckedContinuation { continuation in
            textInserter.insertText(
                text,
                canPaste: canPaste,
                pasteConfirmation: pasteConfirmation
            ) { outcome in
                continuation.resume(returning: outcome)
            }
        }
    }

    private func recoveryError(
        for outcome: TextInsertionOutcome,
        fallback: TextDestinationError
    ) -> TextDestinationError {
        switch outcome {
        case .copiedForManualPaste:
            fallback
        case .insertedConfirmed:
            fallback
        case .pasteDispatchedWithRecovery:
            fallback
        case .unconfirmedAfterClipboardChanged:
            .insertionUnconfirmed
        case .clipboardUnavailable:
            .clipboardUnavailable
        }
    }
}
