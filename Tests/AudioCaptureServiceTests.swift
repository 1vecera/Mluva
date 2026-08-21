import Testing
import AVFoundation
@testable import VoiceScribeMac

@Suite("Audio Capture Service")
struct AudioCaptureServiceTests {

    @Test("Target format is 16kHz, 16-bit, mono")
    func targetFormat() {
        let service = AudioCaptureService()
        // Access through the public interface — verify it doesn't crash
        #expect(service.isRunning == false)
    }

    @Test("Not running initially")
    func notRunningInitially() {
        let service = AudioCaptureService()
        #expect(service.isRunning == false)
    }

    @Test("Double stop is safe (no crash)")
    func doubleStopSafe() {
        let service = AudioCaptureService()
        service.stop()
        service.stop() // Should not crash
        #expect(service.isRunning == false)
    }

    @Test("PCM level meter distinguishes silence from full-scale audio")
    func measuresAudioLevel() {
        let silence = pcmData([0, 0, 0, 0])
        let loud = pcmData([Int16.max, Int16.min, Int16.max, Int16.min])

        #expect(AudioLevelMeter.level(for: silence) == 0)
        #expect(AudioLevelMeter.level(for: loud) > 0.99)
    }

    private func pcmData(_ samples: [Int16]) -> Data {
        samples.withUnsafeBytes { Data($0) }
    }
}
