import Foundation

protocol TextDestination: Sendable {
    func insert(_ delivery: TextDelivery) async throws
}

actor DeliveryCoordinator {
    private let destination: any TextDestination
    private var deliveredEventIDs: Set<String> = []
    private var inFlightEventIDs: Set<String> = []

    init(destination: any TextDestination) {
        self.destination = destination
    }

    func deliver(_ event: TranscriptEvent) async throws -> TextDeliveryOutcome {
        guard event.kind == .final else { return .ignoredVolatile }
        guard !deliveredEventIDs.contains(event.id),
              inFlightEventIDs.insert(event.id).inserted
        else {
            return .ignoredDuplicate
        }
        defer { inFlightEventIDs.remove(event.id) }

        let text = event.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !text.isEmpty else { return .ignoredEmpty }

        try await destination.insert(TextDelivery(id: event.id, text: text))
        deliveredEventIDs.insert(event.id)
        return .inserted
    }
}
