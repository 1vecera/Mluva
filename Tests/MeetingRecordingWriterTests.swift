import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Meeting WAV recording writer")
struct MeetingRecordingWriterTests {
    @Test("Writes private mono PCM WAV data with a finalized header")
    func writesFinalizedWAV() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("meeting-wav-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let writer = MeetingWAVRecordingWriter(directoryURL: directory)
        let id = UUID()
        let filename = try writer.start(id: id)
        let audio = Data([0x01, 0x00, 0xFF, 0x7F])

        try writer.append(audio)
        try writer.finish()

        let url = directory.appendingPathComponent(filename)
        let wav = try Data(contentsOf: url)
        #expect(filename == "\(id.uuidString).wav")
        #expect(wav.count == 44 + audio.count)
        #expect(String(data: wav[0..<4], encoding: .utf8) == "RIFF")
        #expect(uint32(in: wav, at: 4) == UInt32(36 + audio.count))
        #expect(String(data: wav[8..<12], encoding: .utf8) == "WAVE")
        #expect(uint16(in: wav, at: 20) == 1)
        #expect(uint16(in: wav, at: 22) == 1)
        #expect(uint32(in: wav, at: 24) == 16_000)
        #expect(uint16(in: wav, at: 34) == 16)
        #expect(String(data: wav[36..<40], encoding: .utf8) == "data")
        #expect(uint32(in: wav, at: 40) == UInt32(audio.count))
        #expect(wav.suffix(audio.count) == audio)

        let attributes = try FileManager.default.attributesOfItem(atPath: url.path)
        let permissions = attributes[.posixPermissions] as? NSNumber
        #expect(permissions?.intValue == 0o600)
    }

    @Test("Discard removes a completed recording")
    func discardsCompletedRecording() throws {
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("meeting-wav-discard-\(UUID().uuidString)")
        defer { try? FileManager.default.removeItem(at: directory) }
        let writer = MeetingWAVRecordingWriter(directoryURL: directory)
        let filename = try writer.start(id: UUID())
        try writer.append(Data([0x00, 0x00]))
        try writer.finish()
        let url = directory.appendingPathComponent(filename)

        writer.discard()

        #expect(!FileManager.default.fileExists(atPath: url.path))
    }

    private func uint16(in data: Data, at offset: Int) -> UInt16 {
        UInt16(data[offset]) | UInt16(data[offset + 1]) << 8
    }

    private func uint32(in data: Data, at offset: Int) -> UInt32 {
        UInt32(data[offset])
            | UInt32(data[offset + 1]) << 8
            | UInt32(data[offset + 2]) << 16
            | UInt32(data[offset + 3]) << 24
    }
}
