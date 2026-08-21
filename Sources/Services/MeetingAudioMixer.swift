import Foundation

struct MeetingAudioMixer {
    private let chunkBytes: Int
    private var microphoneAudio = Data()
    private var systemAudio = Data()

    init(chunkBytes: Int = 3_200) {
        self.chunkBytes = max(2, chunkBytes - chunkBytes % 2)
    }

    mutating func append(
        _ data: Data,
        from source: MeetingAudioSource
    ) -> [Data] {
        guard !data.isEmpty, data.count.isMultiple(of: 2) else { return [] }
        switch source {
        case .microphone:
            microphoneAudio.append(data)
        case .system:
            systemAudio.append(data)
        }

        var output: [Data] = []
        while microphoneAudio.count >= chunkBytes,
              systemAudio.count >= chunkBytes {
            let microphoneChunk = takeChunk(from: &microphoneAudio)
            let systemChunk = takeChunk(from: &systemAudio)
            output.append(mix(microphoneChunk, systemChunk))
        }
        return output
    }

    mutating func flush() -> [Data] {
        var output: [Data] = []
        while !microphoneAudio.isEmpty || !systemAudio.isEmpty {
            if !microphoneAudio.isEmpty, !systemAudio.isEmpty {
                let bytes = min(
                    chunkBytes,
                    microphoneAudio.count,
                    systemAudio.count
                )
                let microphoneChunk = take(bytes, from: &microphoneAudio)
                let systemChunk = take(bytes, from: &systemAudio)
                output.append(mix(microphoneChunk, systemChunk))
            } else if !microphoneAudio.isEmpty {
                output.append(take(min(chunkBytes, microphoneAudio.count), from: &microphoneAudio))
            } else {
                output.append(take(min(chunkBytes, systemAudio.count), from: &systemAudio))
            }
        }
        return output
    }

    private func takeChunk(from data: inout Data) -> Data {
        take(chunkBytes, from: &data)
    }

    private func take(_ count: Int, from data: inout Data) -> Data {
        let chunk = Data(data.prefix(count))
        data.removeFirst(count)
        return chunk
    }

    private func mix(_ microphone: Data, _ system: Data) -> Data {
        let microphoneSamples = microphone.withUnsafeBytes {
            Array($0.bindMemory(to: Int16.self))
        }
        let systemSamples = system.withUnsafeBytes {
            Array($0.bindMemory(to: Int16.self))
        }
        let count = min(microphoneSamples.count, systemSamples.count)
        let mixed = (0..<count).map { index -> Int16 in
            let average = (
                Int32(microphoneSamples[index]) + Int32(systemSamples[index])
            ) / 2
            return Int16(clamping: average)
        }
        return mixed.withUnsafeBytes { Data($0) }
    }
}
