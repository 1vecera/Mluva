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
            recipient: Recipient(
                socket: guarded ? "/tmp/h.sock" : "", server_epoch: "", pane_id: pane, guard: guarded ? "g" : "",
                name: name, title: "", kind: "claude", project: "/synthetic/\(name)", status: .idle,
                session_id: "s-\(name)-\(pane)-\(pid)", pid: pid),
            display_name: name, draft_only: !guarded, draft_hint: "", can_receive: guarded, spoken_status: "")
    }

    @Test func theFirstSessionSitsAtOneOClockAndLettersStartOnTheHomeRow() {
        let layout = WheelLayout(sessions: ["a", "b", "c"].map { session($0) })
        #expect(layout.slots.map(\.letter) == ["A", "S", "D"])
        #expect(layout.slots[0].angle == 30)
        #expect(layout.slots[1].angle > layout.slots[0].angle)
        #expect(layout.slot(for: "s")?.session.display_name == "b")
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

    @Test func aReplacementInTheSamePaneIsNotTheRememberedSession() {
        var remembered = session("cos", pane: "w1:p1", guarded: true).recipient
        var replacement = remembered
        replacement.guard = "another"
        #expect(!remembered.sameInstance(as: replacement))
        remembered.guard = ""
        replacement = remembered
        replacement.pid = 99
        #expect(!remembered.sameInstance(as: replacement))
    }
}
