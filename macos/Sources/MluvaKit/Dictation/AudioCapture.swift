@preconcurrency import AVFoundation
import Foundation
import os

/// Mluva's capture format: signed little-endian 16-bit mono PCM at 16 kHz.
public enum PCM {
    public static let sampleRate: Double = 16_000
    public static let bytesPerSecond = 32_000
    public static let format = AVAudioFormat(
        commonFormat: .pcmFormatInt16, sampleRate: sampleRate, channels: 1, interleaved: true)!
}

/// Converts whatever the input device delivers into Mluva's PCM.
/// Used only from the capture tap's thread.
public final class PCMConverter: @unchecked Sendable {
    private let converter: AVAudioConverter
    private let input: AVAudioFormat

    public init?(from input: AVAudioFormat) {
        guard let converter = AVAudioConverter(from: input, to: PCM.format) else { return nil }
        self.converter = converter
        self.input = input
    }

    /// Convert one buffer; returns little-endian bytes, possibly empty.
    public func convert(_ buffer: AVAudioPCMBuffer) -> Data {
        let ratio = PCM.sampleRate / input.sampleRate
        let capacity = AVAudioFrameCount((Double(buffer.frameLength) * ratio).rounded(.up)) + 32
        guard let output = AVAudioPCMBuffer(pcmFormat: PCM.format, frameCapacity: capacity) else { return Data() }
        nonisolated(unsafe) var supplied = false
        var error: NSError?
        converter.convert(to: output, error: &error) { _, status in
            if supplied {
                status.pointee = .noDataNow
                return nil
            }
            supplied = true
            status.pointee = .haveData
            return buffer
        }
        guard error == nil, let channel = output.int16ChannelData else { return Data() }
        return Data(bytes: channel[0], count: Int(output.frameLength) * 2)
    }
}

/// Root-mean-square level of 16-bit PCM, 0…1, for the recording light.
public func pcmLevel(_ pcm: Data) -> Float {
    let count = pcm.count / 2
    guard count > 0 else { return 0 }
    let sum = pcm.withUnsafeBytes { raw -> Double in
        raw.bindMemory(to: Int16.self).reduce(0.0) { total, sample in
            let value = Double(Int16(littleEndian: sample)) / 32_768
            return total + value * value
        }
    }
    return Float(min(1, sqrt(sum / Double(count)) * 4))
}

/// Where a recording's audio goes besides the speech provider.
public enum AudioStore: Sendable {
    /// A private WAV file in Mluva's data directory, for batch fallback and retention.
    case file(URL)
    /// Memory only, for Incognito: nothing touches the disk.
    case memory
}

/// A WAV file written as PCM arrives; the header is completed when the recording closes.
public final class WAVWriter {
    public let url: URL
    private let handle: FileHandle
    private var bytes = 0

    public init(url: URL) throws {
        let directory = url.deletingLastPathComponent()
        try FileManager.default.createDirectory(
            at: directory, withIntermediateDirectories: true, attributes: [.posixPermissions: 0o700])
        guard FileManager.default.createFile(
            atPath: url.path, contents: WAVWriter.header(dataBytes: 0),
            attributes: [.posixPermissions: 0o600]) else {
            throw CocoaError(.fileWriteNoPermission)
        }
        self.url = url
        handle = try FileHandle(forWritingTo: url)
        try handle.seekToEnd()
    }

    public func append(_ pcm: Data) throws {
        try handle.write(contentsOf: pcm)
        bytes += pcm.count
    }

    public func close() throws {
        try handle.seek(toOffset: 0)
        try handle.write(contentsOf: WAVWriter.header(dataBytes: bytes))
        try handle.close()
    }

    public static func header(dataBytes: Int) -> Data {
        var data = Data()
        func append<T: FixedWidthInteger>(_ value: T) { withUnsafeBytes(of: value.littleEndian) { data.append(contentsOf: $0) } }
        data.append(contentsOf: Array("RIFF".utf8))
        append(UInt32(36 + dataBytes))
        data.append(contentsOf: Array("WAVEfmt ".utf8))
        append(UInt32(16))
        append(UInt16(1))
        append(UInt16(1))
        append(UInt32(16_000))
        append(UInt32(PCM.bytesPerSecond))
        append(UInt16(2))
        append(UInt16(16))
        data.append(contentsOf: Array("data".utf8))
        append(UInt32(dataBytes))
        return data
    }
}

/// Live microphone capture. The recording counts as started only when the first complete
/// sample arrives; an input that delivers nothing within three seconds fails.
public final class MicrophoneCapture: @unchecked Sendable {
    public typealias Sink = @Sendable (Data) -> Void

    private let engine = AVAudioEngine()
    private let lock = OSAllocatedUnfairLock()
    private var converter: PCMConverter?
    private var sink: Sink?
    private var firstSample: CheckedContinuation<Void, any Error>?
    private var started = false

    public init() {}

    /// Open the default input and wait for its first sample.
    public func start(sink: @escaping Sink) async throws {
        let input = engine.inputNode
        let format = input.outputFormat(forBus: 0)
        guard format.sampleRate > 0, format.channelCount > 0, let converter = PCMConverter(from: format) else {
            throw BridgeError("No microphone is available.")
        }
        lock.withLock {
            self.converter = converter
            self.sink = sink
            self.started = false
        }
        input.installTap(onBus: 0, bufferSize: 1_600, format: format) { [weak self] buffer, _ in
            self?.deliver(buffer)
        }
        engine.prepare()
        try engine.start()
        try await withCheckedThrowingContinuation { (continuation: CheckedContinuation<Void, any Error>) in
            let ready = lock.withLock { () -> Bool in
                if started { return true }
                firstSample = continuation
                return false
            }
            if ready {
                continuation.resume()
                return
            }
            DispatchQueue.global().asyncAfter(deadline: .now() + 3) { [weak self] in
                guard let self else { return }
                let waiting = lock.withLock { () -> CheckedContinuation<Void, any Error>? in
                    defer { firstSample = nil }
                    return firstSample
                }
                waiting?.resume(throwing: BridgeError("The microphone supplied no audio within three seconds."))
            }
        }
    }

    private func deliver(_ buffer: AVAudioPCMBuffer) {
        let (converter, sink) = lock.withLock { (self.converter, self.sink) }
        guard let converter, let sink else { return }
        let pcm = converter.convert(buffer)
        guard !pcm.isEmpty else { return }
        let waiting = lock.withLock { () -> CheckedContinuation<Void, any Error>? in
            started = true
            defer { firstSample = nil }
            return firstSample
        }
        waiting?.resume()
        sink(pcm)
    }

    public func stop() {
        engine.inputNode.removeTap(onBus: 0)
        engine.stop()
        let waiting = lock.withLock { () -> CheckedContinuation<Void, any Error>? in
            sink = nil
            converter = nil
            defer { firstSample = nil }
            return firstSample
        }
        waiting?.resume(throwing: CancellationError())
    }
}
