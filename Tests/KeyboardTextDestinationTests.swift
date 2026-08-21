import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Target-aware text destination")
struct KeyboardTextDestinationTests {
    @Test("Focused target is restored before text insertion")
    func restoresTargetBeforeInsert() async throws {
        let events = OrderedEvents()
        let target = RecordingTextTarget(events: events)
        let inserter = RecordingTextInserter(events: events)
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        try await destination.insert(TextDelivery(id: "segment", text: "hello"))

        #expect(events.values == [
            "restore",
            "replace:hello",
            "restore",
            "insert:hello:true",
            "confirm:hello",
        ])
    }

    @Test("Accessibility-native replacement is preferred over clipboard paste")
    func prefersNativeReplacement() async throws {
        let events = OrderedEvents()
        let target = RecordingTextTarget(events: events, replacesSelection: true)
        let inserter = RecordingTextInserter(events: events)
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        try await destination.insert(TextDelivery(id: "segment", text: "native text"))

        #expect(events.values == [
            "restore",
            "replace:native text",
            "confirm:native text",
        ])
    }

    @Test("Unverified Accessibility replacement falls back to clipboard paste")
    func unverifiedNativeReplacementFallsBackToPaste() async throws {
        let events = OrderedEvents()
        let target = RecordingTextTarget(
            events: events,
            replacesSelection: true,
            confirmsInsertion: false
        )
        let inserter = RecordingTextInserter(
            events: events,
            forcedOutcome: .insertedConfirmed
        )
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        try await destination.insert(TextDelivery(id: "segment", text: "fallback text"))

        #expect(events.values == [
            "restore",
            "replace:fallback text",
            "confirm:fallback text",
            "restore",
            "insert:fallback text:true",
            "confirm:fallback text",
        ])
    }

    @Test("Clipboard-only mode does not require a restorable target")
    func clipboardOnlyDoesNotRequireTarget() async {
        let events = OrderedEvents()
        let inserter = RecordingTextInserter(events: events)
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { false },
            target: nil
        )

        await #expect(throws: TextDestinationError.manualPasteRequired) {
            try await destination.insert(TextDelivery(id: "segment", text: "hello"))
        }

        #expect(events.values == ["insert:hello:false"])
    }

    @Test("Unavailable target blocks insertion")
    func unavailableTargetBlocksInsertion() async {
        let events = OrderedEvents()
        let target = RecordingTextTarget(events: events, canRestore: false)
        let inserter = RecordingTextInserter(events: events)
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        await #expect(throws: TextDestinationError.targetUnavailable) {
            try await destination.insert(TextDelivery(id: "segment", text: "hello"))
        }
        #expect(events.values == ["restore", "insert:hello:false"])
    }

    @Test("Missing captured target copies instead of redirecting insertion")
    func missingTargetUsesRecoveryClipboard() async {
        let events = OrderedEvents()
        let destination = KeyboardTextDestination(
            textInserter: RecordingTextInserter(events: events),
            canPaste: { true },
            target: nil
        )

        await #expect(throws: TextDestinationError.targetUnavailable) {
            try await destination.insert(TextDelivery(id: "segment", text: "hello"))
        }
        #expect(events.values == ["insert:hello:false"])
    }

    @Test("Dispatched paste without target confirmation completes with clipboard recovery")
    func unconfirmedDispatchedPasteCompletes() async throws {
        let events = OrderedEvents()
        let target = RecordingTextTarget(events: events, confirmsInsertion: false)
        let inserter = RecordingTextInserter(
            events: events,
            forcedOutcome: .pasteDispatchedWithRecovery
        )
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        try await destination.insert(TextDelivery(id: "segment", text: "hello"))

        #expect(events.values == [
            "restore",
            "replace:hello",
            "restore",
            "insert:hello:true",
            "confirm:hello",
        ])
    }

    @Test("Unconfirmed paste after clipboard mutation remains a pending delivery")
    func unconfirmedPasteAfterClipboardMutationThrowsRecoveryError() async {
        let events = OrderedEvents()
        let target = RecordingTextTarget(events: events, confirmsInsertion: false)
        let inserter = RecordingTextInserter(
            events: events,
            forcedOutcome: .unconfirmedAfterClipboardChanged
        )
        let destination = KeyboardTextDestination(
            textInserter: inserter,
            canPaste: { true },
            target: target
        )

        await #expect(throws: TextDestinationError.insertionUnconfirmed) {
            try await destination.insert(TextDelivery(id: "segment", text: "hello"))
        }
    }
}

private final class OrderedEvents {
    var values: [String] = []
}

private final class RecordingTextTarget: TextTargetRestoring {
    private let events: OrderedEvents
    private let canRestore: Bool
    private let replacesSelection: Bool
    private let confirmsInsertion: Bool

    init(
        events: OrderedEvents,
        canRestore: Bool = true,
        replacesSelection: Bool = false,
        confirmsInsertion: Bool = true
    ) {
        self.events = events
        self.canRestore = canRestore
        self.replacesSelection = replacesSelection
        self.confirmsInsertion = confirmsInsertion
    }

    @discardableResult
    func restore() -> Bool {
        events.values.append("restore")
        return canRestore
    }

    func replaceSelection(with text: String) -> Bool {
        events.values.append("replace:\(text)")
        return replacesSelection
    }

    func confirmInsertion(of text: String) -> Bool {
        events.values.append("confirm:\(text)")
        return confirmsInsertion
    }
}

private final class RecordingTextInserter: TextInserting {
    private let events: OrderedEvents
    private let forcedOutcome: TextInsertionOutcome?

    init(
        events: OrderedEvents,
        forcedOutcome: TextInsertionOutcome? = nil
    ) {
        self.events = events
        self.forcedOutcome = forcedOutcome
    }

    func insertText(
        _ text: String,
        canPaste: Bool,
        pasteConfirmation: (() -> Bool)?,
        completion: @escaping (TextInsertionOutcome) -> Void
    ) {
        events.values.append("insert:\(text):\(canPaste)")
        let confirmed = pasteConfirmation?() ?? false
        completion(forcedOutcome ?? (canPaste && confirmed
            ? .insertedConfirmed
            : .copiedForManualPaste))
    }
}
