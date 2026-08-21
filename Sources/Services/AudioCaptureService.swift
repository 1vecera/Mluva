import AVFoundation
import Foundation

protocol AudioCapturing: AnyObject {
    var onAudioChunk: ((Data) -> Void)? { get set }

    func start() throws
    func stop()
}

enum AudioCaptureError: LocalizedError {
    case noInputDevice
    case engineStartFailed(Error)

    var errorDescription: String? {
        switch self {
        case .noInputDevice:
            return "No audio input device found. Check microphone permissions."
        case .engineStartFailed(let err):
            return "Audio engine failed to start: \(err.localizedDescription)"
        }
    }
}

struct AudioLevelMeter {
    static func level(for pcm16Audio: Data) -> Double {
        guard pcm16Audio.count >= MemoryLayout<Int16>.size else { return 0 }
        return pcm16Audio.withUnsafeBytes { bytes in
            let samples = bytes.bindMemory(to: Int16.self)
            guard !samples.isEmpty else { return 0 }
            let squareSum = samples.reduce(0.0) { total, sample in
                let normalized = Double(sample) / 32_768
                return total + normalized * normalized
            }
            return min(1, sqrt(squareSum / Double(samples.count)))
        }
    }
}

final class AudioCaptureService {
    static var currentInputDeviceName: String? {
        AVCaptureDevice.default(for: .audio)?.localizedName
    }

    private let engine = AVAudioEngine()
    private var converter: AVAudioConverter?
    private let targetFormat: AVAudioFormat
    private let chunkSize = 3200 // 100ms @ 16kHz/16-bit/mono = 1600 samples * 2 bytes
    private var buffer = Data()
    private var isCapturing = false

    var onAudioChunk: ((Data) -> Void)?

    init() {
        targetFormat = AVAudioFormat(
            commonFormat: .pcmFormatInt16,
            sampleRate: 16000.0,
            channels: 1,
            interleaved: true
        )!
    }

    func start() throws {
        guard !isCapturing else { return }

        let inputNode = engine.inputNode
        let inputFormat = inputNode.inputFormat(forBus: 0)

        guard inputFormat.sampleRate > 0 else {
            throw AudioCaptureError.noInputDevice
        }

        converter = AVAudioConverter(from: inputFormat, to: targetFormat)

        inputNode.installTap(onBus: 0, bufferSize: 4096, format: nil) { [weak self] pcmBuffer, _ in
            self?.processBuffer(pcmBuffer)
        }

        engine.prepare()

        do {
            try engine.start()
        } catch {
            engine.inputNode.removeTap(onBus: 0)
            throw AudioCaptureError.engineStartFailed(error)
        }

        isCapturing = true
    }

    func stop() {
        guard isCapturing else { return }
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        isCapturing = false

        // Flush remaining buffer
        if !buffer.isEmpty {
            onAudioChunk?(buffer)
            buffer.removeAll()
        }
    }

    private func processBuffer(_ inputBuffer: AVAudioPCMBuffer) {
        guard let converter = converter else { return }

        let ratio = targetFormat.sampleRate / inputBuffer.format.sampleRate
        let capacity = UInt32(Double(inputBuffer.frameLength) * ratio) + 1

        guard let pcmBuffer = AVAudioPCMBuffer(
            pcmFormat: targetFormat,
            frameCapacity: capacity
        ) else { return }

        var error: NSError?
        var hasData = true

        converter.convert(to: pcmBuffer, error: &error) { _, outStatus in
            if hasData {
                hasData = false
                outStatus.pointee = .haveData
                return inputBuffer
            } else {
                outStatus.pointee = .noDataNow
                return nil
            }
        }

        guard error == nil,
              let channelData = pcmBuffer.int16ChannelData?[0],
              pcmBuffer.frameLength > 0 else { return }

        let byteCount = Int(pcmBuffer.frameLength) * 2
        let data = Data(bytes: channelData, count: byteCount)

        buffer.append(data)

        // Emit complete 3200-byte chunks (100ms each)
        while buffer.count >= chunkSize {
            let chunk = buffer.prefix(chunkSize)
            onAudioChunk?(Data(chunk))
            buffer.removeFirst(chunkSize)
        }
    }

    var isRunning: Bool { isCapturing }
}

extension AudioCaptureService: AudioCapturing {}
