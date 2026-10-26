import CoreGraphics
@preconcurrency import AVFoundation
import Foundation
import Testing
@testable import MluvaKit

struct AudioTests {
    /// A synthetic 48 kHz stereo float sine, like a typical Mac input device.
    func sine(seconds: Double, rate: Double = 48_000) -> AVAudioPCMBuffer {
        let format = AVAudioFormat(standardFormatWithSampleRate: rate, channels: 2)!
        let frames = AVAudioFrameCount(seconds * rate)
        let buffer = AVAudioPCMBuffer(pcmFormat: format, frameCapacity: frames)!
        buffer.frameLength = frames
        for channel in 0..<2 {
            for frame in 0..<Int(frames) {
                buffer.floatChannelData![channel][frame] = 0.5 * sin(2 * .pi * 440 * Float(frame) / Float(rate))
            }
        }
        return buffer
    }

    @Test func inputIsConvertedToSixteenKilohertzMonoSixteenBit() throws {
        let converter = try #require(PCMConverter(from: sine(seconds: 0.1).format))
        var pcm = Data()
        for _ in 0..<10 { pcm.append(converter.convert(sine(seconds: 0.1))) }
        // One second of input becomes about 32,000 bytes of PCM.
        #expect(abs(pcm.count - PCM.bytesPerSecond) < 1_000)
        #expect(pcmLevel(pcm) > 0.5)
        #expect(pcmLevel(Data(count: 3_200)) == 0)
    }

    @Test func wavFilesArePrivateAndTheirHeaderIsCompleted() throws {
        let url = FileManager.default.temporaryDirectory.appending(path: "mluva-\(UUID().uuidString)/a.wav")
        defer { try? FileManager.default.removeItem(at: url.deletingLastPathComponent()) }
        let writer = try WAVWriter(url: url)
        try writer.append(Data(repeating: 1, count: 640))
        try writer.close()
        let data = try Data(contentsOf: url)
        #expect(data.count == 44 + 640)
        #expect(data.prefix(4) == Data("RIFF".utf8))
        let size = data[40..<44].withUnsafeBytes { $0.loadUnaligned(as: UInt32.self) }
        #expect(UInt32(littleEndian: size) == 640)
        let mode = try FileManager.default.attributesOfItem(atPath: url.path)[.posixPermissions] as? Int
        #expect(mode == 0o600)
    }
}

struct WheelTests {
    func session(_ name: String, pane: String = "", guarded: Bool = false, pid: UInt32 = 1) -> AgentSession {
        AgentSession(
            target: AgentTarget(
                socket: guarded ? "/tmp/h.sock" : "", pane_id: pane, guard: guarded ? "g" : "",
                name: name, kind: "claude", project: "/synthetic/\(name)", status: .idle,
                session_id: "s-\(name)-\(pane)-\(pid)", pid: pid),
            display_name: name, delivery: guarded ? "send" : "draft_only", verb: guarded ? "Send" : "Draft only",
            draft_only: !guarded, draft_hint: "", can_receive: guarded, spoken_status: "")
    }

    @Test func theFirstSessionSitsAtOneOClockAndLettersStartOnTheHomeRow() {
        let layout = WheelLayout(sessions: ["a", "b", "c"].map { session($0) })
        #expect(layout.slots.map(\.letter) == ["A", "S", "D"])
        #expect(layout.slots[0].angle == 30)
        #expect(layout.slots[1].angle > layout.slots[0].angle)
        #expect(layout.slot(for: "s")?.session.display_name == "b")
    }

    @Test func capsulesHugTheCentreWithoutTouchingEachOtherOrTheDisc() {
        for count in [1, 3, 7, 12, 26] {
            let layout = WheelLayout(sessions: (0..<count).map { session("session \($0)", pid: UInt32($0)) })
            let sizes = layout.slots.map { CGSize(width: 150 + Double($0.index % 3) * 60, height: 44) }
            let centres = layout.offsets(itemSizes: sizes, centerRadius: 118, limit: 2_000)
            let rects = zip(centres, sizes).map { CGRect(x: $0.x - $1.width / 2, y: $0.y - $1.height / 2, width: $1.width, height: $1.height) }
            for (i, rect) in rects.enumerated() {
                // The nearest point of the capsule stays outside the disc.
                let nearest = CGPoint(x: min(max(0, rect.minX), rect.maxX), y: min(max(0, rect.minY), rect.maxY))
                #expect(hypot(nearest.x, nearest.y) >= 118, "slot \(i) of \(count) overlaps the centre")
                for other in rects[(i + 1)...] where rect.intersects(other) {
                    Issue.record("slots overlap in a ring of \(count): \(rect) and \(other)")
                }
            }
            // A small ring stays compact: the first capsule sits close to the disc.
            if count <= 7 { #expect(hypot(centres[0].x, centres[0].y) < 300) }
        }
    }

    @Test func moreThanTwentySixSessionsPage() {
        let sessions = (0..<30).map { session("s\($0)", pid: UInt32($0)) }
        let first = WheelLayout(sessions: sessions)
        #expect(first.slots.count == 26)
        #expect(first.pageCount == 2)
        let second = WheelLayout(sessions: sessions, page: 1)
        #expect(second.slots.count == 4)
        #expect(second.slots[0].letter == "A")
        #expect(second.slots[0].index == 26)
    }

    @Test func onlyDuplicateNamesShowTheirHandle() {
        let layout = WheelLayout(sessions: [session("cos", pane: "w1:p1"), session("cos", pane: "w1:p2"), session("notes")])
        #expect(layout.slots[0].disambiguator == "w1:p1")
        #expect(layout.slots[2].disambiguator == nil)
    }

    /// The core's identity rules: a session is its product, session ID and process, whatever
    /// route reaches it now; Herdr's guard decides only without a session on both sides.
    @Test func aReplacementInTheSamePaneIsNotTheRememberedSession() {
        let remembered = session("cos", pane: "w1:p1", guarded: true).target
        var unguarded = remembered
        unguarded.guard = ""
        #expect(remembered.sameInstance(as: unguarded), "Herdr's guard going away keeps the choice")
        var restarted = remembered
        restarted.pid = 99
        #expect(!remembered.sameInstance(as: restarted))
        var reused = remembered
        (reused.started, restarted.started) = ("Mon Oct 26 11:10:03 2026", "Mon Oct 26 11:10:04 2026")
        restarted.pid = remembered.pid
        #expect(!reused.sameInstance(as: restarted), "a reused PID is another process")
        var pane = remembered
        pane.session_id = ""
        var replaced = pane
        replaced.guard = "another"
        #expect(!pane.sameInstance(as: replaced), "without sessions, Herdr's guard decides")
        let chat = AgentTarget(name: "claude-new-chat", kind: "claude_desktop", status: .idle, app: "claude_new_chat")
        #expect(chat.sameInstance(as: chat) && !chat.sameInstance(as: remembered))
    }
}
