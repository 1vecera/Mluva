import Foundation
import Testing
@testable import VoiceScribeMac

@Suite("Meeting audio mixer")
struct MeetingAudioMixerTests {
    @Test("Aligned microphone and system samples mix without clipping")
    func mixesAlignedSources() {
        var mixer = MeetingAudioMixer(chunkBytes: 4)

        #expect(mixer.append(pcm16([1_000, -1_000]), from: .microphone).isEmpty)
        let mixed = mixer.append(pcm16([3_000, -5_000]), from: .system)

        #expect(mixed.map(samples) == [[2_000, -3_000]])
    }

    @Test("Unpaired source audio remains intelligible when capture stops")
    func flushesUnpairedAudio() {
        var mixer = MeetingAudioMixer(chunkBytes: 4)
        _ = mixer.append(pcm16([2_000, -2_000]), from: .microphone)

        let flushed = mixer.flush()

        #expect(flushed.map(samples) == [[2_000, -2_000]])
    }

    @Test("Mixed samples saturate instead of wrapping")
    func saturatesSamples() {
        var mixer = MeetingAudioMixer(chunkBytes: 4)
        _ = mixer.append(pcm16([Int16.max, Int16.min]), from: .microphone)

        let mixed = mixer.append(
            pcm16([Int16.max, Int16.min]),
            from: .system
        )

        #expect(mixed.map(samples) == [[Int16.max, Int16.min]])
    }

    @Test("Coordinator starts both explicit meeting sources and emits their mix")
    func coordinatesBothSources() async throws {
        let microphone = FakeMeetingAudioCapture()
        let system = FakeMeetingAudioCapture()
        let coordinator = MeetingAudioCaptureCoordinator(
            microphone: microphone,
            system: system,
            chunkBytes: 4
        )
        var output: [Data] = []
        coordinator.onAudioChunk = { output.append($0) }

        try await coordinator.start()
        microphone.emit(pcm16([1_000, -1_000]))
        system.emit(pcm16([3_000, -5_000]))
        await coordinator.stop()

        #expect(microphone.startCount == 1)
        #expect(system.startCount == 1)
        #expect(microphone.stopCount == 1)
        #expect(system.stopCount == 1)
        #expect(output.map(samples) == [[2_000, -3_000]])
    }

    private func pcm16(_ samples: [Int16]) -> Data {
        samples.withUnsafeBytes { Data($0) }
    }

    private func samples(_ data: Data) -> [Int16] {
        data.withUnsafeBytes { Array($0.bindMemory(to: Int16.self)) }
    }
}

private final class FakeMeetingAudioCapture: MeetingAudioSourceCapturing {
    var onAudioChunk: ((Data) -> Void)?
    var onError: ((any Error) -> Void)?
    private(set) var startCount = 0
    private(set) var stopCount = 0

    func start() async throws {
        startCount += 1
    }

    func stop() async {
        stopCount += 1
    }

    func emit(_ data: Data) {
        onAudioChunk?(data)
    }
}
