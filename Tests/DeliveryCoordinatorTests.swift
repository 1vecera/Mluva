import Testing
@testable import VoiceScribeMac

@Suite("Final text delivery")
struct DeliveryCoordinatorTests {
    @Test("Volatile recognition never reaches the target application")
    func volatileTextIsNotDelivered() async throws {
        let destination = RecordingTextDestination()
        let coordinator = DeliveryCoordinator(destination: destination)

        let outcome = try await coordinator.deliver(.volatile(id: "segment-a", text: "hello wor"))

        #expect(outcome == .ignoredVolatile)
        #expect(await destination.deliveries.isEmpty)
    }

    @Test("Final recognition is inserted exactly once")
    func finalTextIsDeliveredOnce() async throws {
        let destination = RecordingTextDestination()
        let coordinator = DeliveryCoordinator(destination: destination)
        let event = TranscriptEvent.final(id: "segment-a", text: "hello world")

        #expect(try await coordinator.deliver(event) == .inserted)
        #expect(try await coordinator.deliver(event) == .ignoredDuplicate)
        #expect(await destination.deliveries == [TextDelivery(id: "segment-a", text: "hello world")])
    }

    @Test("Failed delivery can be retried without being marked complete")
    func failedDeliveryCanRetry() async throws {
        let destination = RecordingTextDestination(failuresBeforeSuccess: 1)
        let coordinator = DeliveryCoordinator(destination: destination)
        let event = TranscriptEvent.final(id: "segment-a", text: "retry me")

        await #expect(throws: RecordingTextDestination.Failure.simulated) {
            try await coordinator.deliver(event)
        }
        #expect(try await coordinator.deliver(event) == .inserted)
        #expect(await destination.deliveries == [TextDelivery(id: "segment-a", text: "retry me")])
    }

    @Test("Concurrent final events share one insertion")
    func concurrentFinalEventsAreDeliveredOnce() async throws {
        let destination = SuspendedTextDestination()
        let coordinator = DeliveryCoordinator(destination: destination)
        let event = TranscriptEvent.final(id: "segment-a", text: "only once")

        let first = Task { try await coordinator.deliver(event) }
        await destination.waitUntilInsertionStarts()
        let duplicate = try await coordinator.deliver(event)
        await destination.completeInsertion()

        #expect(duplicate == .ignoredDuplicate)
        #expect(try await first.value == .inserted)
        #expect(await destination.deliveries == [
            TextDelivery(id: "segment-a", text: "only once")
        ])
    }
}

private actor RecordingTextDestination: TextDestination {
    enum Failure: Error {
        case simulated
    }

    private var remainingFailures: Int
    private(set) var deliveries: [TextDelivery] = []

    init(failuresBeforeSuccess: Int = 0) {
        remainingFailures = failuresBeforeSuccess
    }

    func insert(_ delivery: TextDelivery) async throws {
        if remainingFailures > 0 {
            remainingFailures -= 1
            throw Failure.simulated
        }
        deliveries.append(delivery)
    }
}

private actor SuspendedTextDestination: TextDestination {
    private(set) var deliveries: [TextDelivery] = []
    private var insertionContinuation: CheckedContinuation<Void, Never>?
    private var insertionStarted = false

    func insert(_ delivery: TextDelivery) async throws {
        insertionStarted = true
        await withCheckedContinuation { insertionContinuation = $0 }
        deliveries.append(delivery)
    }

    func waitUntilInsertionStarts() async {
        while !insertionStarted {
            await Task.yield()
        }
    }

    func completeInsertion() {
        insertionContinuation?.resume()
        insertionContinuation = nil
    }
}
