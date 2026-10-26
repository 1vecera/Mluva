import Foundation

/// The F12 session wheel's arrangement: up to 26 sessions per page, the first at one o'clock
/// and the rest clockwise, each with a letter key in home-row-first order.
public struct WheelLayout: Sendable, Equatable {
    public static let letters = Array("ASDFGHJKLQWERTYUIOPZXCVBNM")
    public static let pageSize = letters.count

    public struct Slot: Sendable, Equatable, Identifiable {
        public let index: Int
        public let letter: Character
        /// Clockwise from twelve o'clock, in degrees.
        public let angle: Double
        public let session: AgentSession
        /// Shown only when another session on the page has the same name.
        public let disambiguator: String?

        public var id: String { session.id }
    }

    public let page: Int
    public let pageCount: Int
    public let slots: [Slot]

    public init(sessions: [AgentSession], page requested: Int = 0) {
        let pageCount = max(1, (sessions.count + Self.pageSize - 1) / Self.pageSize)
        let page = min(max(0, requested), pageCount - 1)
        let visible = Array(sessions.dropFirst(page * Self.pageSize).prefix(Self.pageSize))
        let counts = Dictionary(grouping: visible, by: \.display_name).mapValues(\.count)
        // One o'clock for the first session; the ring is shared evenly from there.
        let step = visible.isEmpty ? 0 : 360.0 / Double(max(visible.count, 6))
        self.page = page
        self.pageCount = pageCount
        slots = visible.enumerated().map { offset, session in
            let duplicate = (counts[session.display_name] ?? 0) > 1
            let handle = session.recipient.pane_id.isEmpty ? session.recipient.name : session.recipient.pane_id
            return Slot(
                index: page * Self.pageSize + offset,
                letter: Self.letters[offset],
                angle: (30 + Double(offset) * step).truncatingRemainder(dividingBy: 360),
                session: session,
                disambiguator: duplicate ? handle : nil)
        }
    }

    /// The slot a letter key chooses on this page.
    public func slot(for key: Character) -> Slot? {
        let key = Character(key.uppercased())
        return slots.first { $0.letter == key }
    }

    /// Ring radius that keeps every capsule clear of the centre disc and of its neighbours,
    /// within the screen.
    public func radius(itemWidth: Double, centerRadius: Double, limit: Double) -> Double {
        let count = Double(max(slots.count, 6))
        // Clear of the centre even for a capsule lying sideways at three or nine o'clock.
        let clearOfCenter = centerRadius + itemWidth / 2 + 28
        // Neighbours a chord apart; capsules are wide but short, so most of the width suffices.
        let clearOfNeighbours = (itemWidth * 0.7 + 24) / (2 * sin(.pi / count))
        return min(max(clearOfCenter, clearOfNeighbours), limit)
    }
}
