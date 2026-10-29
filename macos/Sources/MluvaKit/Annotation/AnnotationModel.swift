import Foundation

/// The shared annotation model as the bridge reads it. Times are milliseconds on the recording
/// clock; coordinates are logical pixels local to their screen, origin top left.
public enum Annotation {
    public struct Screen: Codable, Equatable, Sendable {
        public var name: String
        public var x: Int
        public var y: Int
        public var width: Int
        public var height: Int
        public var scale: Double
    }

    public struct Point: Codable, Equatable, Sendable {
        public var x: Double
        public var y: Double
        public var t: UInt64

        public init(x: Double, y: Double, t: UInt64) {
            self.x = x
            self.y = y
            self.t = t
        }
    }

    /// One key-held pointer path, or a pin when `pin` is set.
    public struct Stroke: Codable, Equatable, Sendable {
        public var t0: UInt64
        public var t1: UInt64
        public var screen: String
        public var pin: UInt32?
        public var points: [Point]

        public init(t0: UInt64, t1: UInt64, screen: String, pin: UInt32?, points: [Point]) {
            self.t0 = t0
            self.t1 = t1
            self.screen = screen
            self.pin = pin
            self.points = points
        }
    }

    public struct WindowBox: Codable, Equatable, Sendable {
        public var rect: [Double]
        public var active: Bool

        public init(rect: [Double], active: Bool) {
            self.rect = rect
            self.active = active
        }
    }

    public enum FrameReason: String, Codable, Sendable {
        case start, sample, mark, window
    }

    /// A captured screen. `path` only names it: raw frames stay in memory.
    public struct Frame: Codable, Equatable, Sendable {
        public var t: UInt64
        public var screen: String
        public var reason: FrameReason
        public var path: String
        public var width: Int
        public var height: Int
        public var windows: [WindowBox]

        public init(t: UInt64, screen: String, reason: FrameReason, path: String, width: Int, height: Int, windows: [WindowBox]) {
            self.t = t
            self.screen = screen
            self.reason = reason
            self.path = path
            self.width = width
            self.height = height
            self.windows = windows
        }
    }

    public enum Event: Codable, Equatable, Sendable {
        /// The application's name only; window titles are never kept.
        case window(t: UInt64, name: String)
        case workspace(t: UInt64, name: String)

        private enum Keys: String, CodingKey { case kind, t, `class`, name }

        public init(from decoder: Decoder) throws {
            let container = try decoder.container(keyedBy: Keys.self)
            let t = try container.decode(UInt64.self, forKey: .t)
            if try container.decode(String.self, forKey: .kind) == "window" {
                self = .window(t: t, name: try container.decode(String.self, forKey: .class))
            } else {
                self = .workspace(t: t, name: try container.decode(String.self, forKey: .name))
            }
        }

        public func encode(to encoder: Encoder) throws {
            var container = encoder.container(keyedBy: Keys.self)
            switch self {
            case .window(let t, let name):
                try container.encode("window", forKey: .kind)
                try container.encode(t, forKey: .t)
                try container.encode(name, forKey: .class)
            case .workspace(let t, let name):
                try container.encode("workspace", forKey: .kind)
                try container.encode(t, forKey: .t)
                try container.encode(name, forKey: .name)
            }
        }
    }

    public struct Capture: Codable, Equatable, Sendable {
        public var duration_ms: UInt64 = 0
        public var screens: [Screen] = []
        public var strokes: [Stroke] = []
        public var frames: [Frame] = []
        public var events: [Event] = []
        /// `[t, words shown]` whenever the live preview grew.
        public var preview: [[UInt64]] = []

        public init() {}

        public var hasMarks: Bool { strokes.contains { !$0.points.isEmpty } }
    }

    public struct Word: Codable, Equatable, Sendable {
        public var t0: UInt64
        public var t1: UInt64
        public var text: String
    }

    public struct Mark: Codable, Equatable, Sendable {
        public var id: String
        public var strokes: [Int]
        public var t0: UInt64
        public var t1: UInt64
        public var screen: String
        public var region: [Double]
        public var shape: String
        public var pin: UInt32?
        public var description: String
    }

    public struct Keyframe: Codable, Equatable, Sendable {
        public var index: Int
        public var t: UInt64
        public var reason: String
        public var frame: Int
        public var event: String
        public var marks: [Int]
        public var said: String?
    }

    /// What the planner chose: each keyframe with its crop and the file to draw it into.
    public struct Plan: Decodable, Sendable {
        public struct Item: Decodable, Sendable {
            public var keyframe: Keyframe
            public var crop: [Double]
            public var file: String
        }

        public var identifier: String
        public var marks: [Mark]
        public var keyframes: [Item]
        public var contact_sheet: String
    }

    /// A finished bundle in History.
    public struct Bundle: Decodable, Equatable, Sendable, Identifiable {
        public var identifier: String
        public var created_at: String
        public var directory: String
        public var keyframes: [String]
        public var contact_sheet: String

        public var id: String { identifier }
    }

    /// Provider word times in milliseconds, dropping any that are not finite.
    public static func words(_ result: TranscriptionResult) -> [Word] {
        (result.words ?? []).compactMap { word in
            guard word.start_seconds.isFinite, word.end_seconds.isFinite else { return nil }
            let start = max(0, word.start_seconds)
            let end = max(start, word.end_seconds)
            return Word(t0: UInt64((start * 1_000).rounded()), t1: UInt64((end * 1_000).rounded()), text: word.text)
        }
    }

    /// `M:SS`, as the badges and summary show it.
    public static func clock(_ ms: UInt64) -> String {
        let seconds = ms / 1_000
        return String(format: "%d:%02d", seconds / 60, seconds % 60)
    }
}
