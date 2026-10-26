import CoreGraphics
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
            let handle = session.target.pane_id.isEmpty ? session.target.name : session.target.pane_id
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

    /// Where each slot's capsule centre sits relative to the wheel's centre, y down: one ring,
    /// as small as keeps every capsule clear of the centre disc and of every other capsule.
    public func offsets(itemSizes: [CGSize], centerRadius: Double, gap: Double = 18, limit: Double) -> [CGPoint] {
        guard itemSizes.count == slots.count, !slots.isEmpty else { return [] }
        // A capsule's centre must be this far out for its nearest edge to clear the disc.
        let clear = zip(slots, itemSizes).map { slot, size in
            let radians = slot.angle * .pi / 180
            return centerRadius + gap + abs(sin(radians)) * size.width / 2 + abs(cos(radians)) * size.height / 2
        }
        func place(_ radius: Double) -> [CGRect] {
            zip(slots, itemSizes).map { slot, size in
                let radians = slot.angle * .pi / 180
                return CGRect(
                    x: radius * sin(radians) - size.width / 2, y: -radius * cos(radians) - size.height / 2,
                    width: size.width, height: size.height)
            }
        }
        // At most 26 capsules, so checking every pair is cheap.
        func crowded(_ rects: [CGRect]) -> Bool {
            rects.indices.contains { i in rects[(i + 1)...].contains { rects[i].insetBy(dx: -5, dy: -5).intersects($0) } }
        }
        var radius = clear.max() ?? centerRadius
        var rects = place(radius)
        while radius < limit, crowded(rects) {
            radius += 4
            rects = place(radius)
        }
        return rects.map { CGPoint(x: $0.midX, y: $0.midY) }
    }
}
