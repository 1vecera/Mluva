import AVFoundation
import CoreMedia
import Foundation
import ScreenCaptureKit

protocol MeetingAudioSourceCapturing: AnyObject {
    var onAudioChunk: ((Data) -> Void)? { get set }
    var onError: ((Error) -> Void)? { get set }

    func start() async throws
    func stop() async
}

final class MicrophoneMeetingAudioCapture: MeetingAudioSourceCapturing {
    var onAudioChunk: ((Data) -> Void)? {
        didSet { capture.onAudioChunk = onAudioChunk }
    }
    var onError: ((Error) -> Void)?

    private let capture: any AudioCapturing

    init(capture: any AudioCapturing = AudioCaptureService()) {
        self.capture = capture
        self.capture.onAudioChunk = onAudioChunk
    }

    func start() async throws {
        try capture.start()
    }

    func stop() async {
        capture.stop()
    }
}

enum MeetingSystemAudioError: Error, LocalizedError {
    case noDisplay
    case streamConfigurationFailed

    var errorDescription: String? {
        switch self {
        case .noDisplay:
            "No display is available for system-audio capture."
        case .streamConfigurationFailed:
            "macOS could not prepare system-audio capture. Check Screen Recording permission."
        }
    }
}

final class SystemMeetingAudioCapture: NSObject, MeetingAudioSourceCapturing, @unchecked Sendable {
    var onAudioChunk: ((Data) -> Void)?
    var onError: ((Error) -> Void)?

    private let outputQueue = DispatchQueue(
        label: "com.voicescribe.meeting-system-audio",
        qos: .userInteractive
    )
    private let lock = NSLock()
    private let targetFormat = AVAudioFormat(
        commonFormat: .pcmFormatInt16,
        sampleRate: 16_000,
        channels: 1,
        interleaved: true
    )!
    private var stream: SCStream?
    private var converter: AVAudioConverter?
    private var bufferedAudio = Data()

    func start() async throws {
        let content = try await SCShareableContent.excludingDesktopWindows(
            false,
            onScreenWindowsOnly: true
        )
        guard let display = content.displays.first else {
            throw MeetingSystemAudioError.noDisplay
        }
        let filter = SCContentFilter(
            display: display,
            excludingApplications: [],
            exceptingWindows: []
        )
        let configuration = SCStreamConfiguration()
        configuration.width = 2
        configuration.height = 2
        configuration.minimumFrameInterval = CMTime(value: 1, timescale: 1)
        configuration.queueDepth = 3
        configuration.capturesAudio = true
        configuration.excludesCurrentProcessAudio = true
        configuration.sampleRate = 16_000
        configuration.channelCount = 1

        let stream = SCStream(
            filter: filter,
            configuration: configuration,
            delegate: self
        )
        do {
            try stream.addStreamOutput(
                self,
                type: .audio,
                sampleHandlerQueue: outputQueue
            )
            lock.withLock {
                self.stream = stream
                converter = nil
                bufferedAudio.removeAll(keepingCapacity: true)
            }
            try await stream.startCapture()
        } catch {
            lock.withLock { self.stream = nil }
            throw MeetingSystemAudioError.streamConfigurationFailed
        }
    }

    func stop() async {
        let activeStream = lock.withLock { () -> SCStream? in
            let active = stream
            stream = nil
            return active
        }
        if let activeStream {
            try? await activeStream.stopCapture()
        }
        outputQueue.sync { [weak self] in
            self?.flushBufferedAudio()
            self?.converter = nil
        }
    }

    private func process(_ sampleBuffer: CMSampleBuffer) {
        guard CMSampleBufferDataIsReady(sampleBuffer),
              let formatDescription = CMSampleBufferGetFormatDescription(sampleBuffer)
        else {
            return
        }
        let inputFormat = AVAudioFormat(
            cmAudioFormatDescription: formatDescription
        )
        guard
              let inputBuffer = pcmBuffer(
                from: sampleBuffer,
                format: inputFormat
              ),
              let converted = convert(inputBuffer)
        else {
            return
        }
        let buffers = UnsafeMutableAudioBufferListPointer(
            converted.mutableAudioBufferList
        )
        guard let firstBuffer = buffers.first,
              let bytes = firstBuffer.mData,
              firstBuffer.mDataByteSize > 0
        else {
            return
        }
        bufferedAudio.append(
            bytes.assumingMemoryBound(to: UInt8.self),
            count: Int(firstBuffer.mDataByteSize)
        )
        while bufferedAudio.count >= 3_200 {
            let chunk = Data(bufferedAudio.prefix(3_200))
            bufferedAudio.removeFirst(3_200)
            onAudioChunk?(chunk)
        }
    }

    private func pcmBuffer(
        from sampleBuffer: CMSampleBuffer,
        format: AVAudioFormat
    ) -> AVAudioPCMBuffer? {
        var requiredSize = 0
        let flags = UInt32(kCMSampleBufferFlag_AudioBufferList_Assure16ByteAlignment)
        let sizeStatus = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: &requiredSize,
            bufferListOut: nil,
            bufferListSize: 0,
            blockBufferAllocator: nil,
            blockBufferMemoryAllocator: nil,
            flags: flags,
            blockBufferOut: nil
        )
        guard sizeStatus == noErr || requiredSize > 0 else { return nil }

        let rawList = UnsafeMutableRawPointer.allocate(
            byteCount: requiredSize,
            alignment: MemoryLayout<AudioBufferList>.alignment
        )
        defer { rawList.deallocate() }
        let audioBufferList = rawList.bindMemory(
            to: AudioBufferList.self,
            capacity: 1
        )
        var retainedBlockBuffer: CMBlockBuffer?
        let status = CMSampleBufferGetAudioBufferListWithRetainedBlockBuffer(
            sampleBuffer,
            bufferListSizeNeededOut: nil,
            bufferListOut: audioBufferList,
            bufferListSize: requiredSize,
            blockBufferAllocator: kCFAllocatorDefault,
            blockBufferMemoryAllocator: kCFAllocatorDefault,
            flags: flags,
            blockBufferOut: &retainedBlockBuffer
        )
        guard status == noErr,
              let borrowedBuffer = AVAudioPCMBuffer(
                pcmFormat: format,
                bufferListNoCopy: audioBufferList,
                deallocator: nil
              ),
              let ownedBuffer = AVAudioPCMBuffer(
                pcmFormat: format,
                frameCapacity: AVAudioFrameCount(
                    CMSampleBufferGetNumSamples(sampleBuffer)
                )
              )
        else {
            return nil
        }
        borrowedBuffer.frameLength = ownedBuffer.frameCapacity
        ownedBuffer.frameLength = ownedBuffer.frameCapacity
        let sourceBuffers = UnsafeMutableAudioBufferListPointer(
            borrowedBuffer.mutableAudioBufferList
        )
        let destinationBuffers = UnsafeMutableAudioBufferListPointer(
            ownedBuffer.mutableAudioBufferList
        )
        guard sourceBuffers.count == destinationBuffers.count else { return nil }
        for index in sourceBuffers.indices {
            guard let source = sourceBuffers[index].mData,
                  let destination = destinationBuffers[index].mData
            else {
                return nil
            }
            let byteCount = min(
                Int(sourceBuffers[index].mDataByteSize),
                Int(destinationBuffers[index].mDataByteSize)
            )
            destination.copyMemory(from: source, byteCount: byteCount)
            destinationBuffers[index].mDataByteSize = UInt32(byteCount)
        }
        return ownedBuffer
    }

    private func convert(_ input: AVAudioPCMBuffer) -> AVAudioPCMBuffer? {
        if input.format == targetFormat {
            return input
        }
        let converter: AVAudioConverter
        if let existing = self.converter,
           existing.inputFormat == input.format {
            converter = existing
        } else {
            guard let created = AVAudioConverter(
                from: input.format,
                to: targetFormat
            ) else {
                return nil
            }
            created.primeMethod = .none
            self.converter = created
            converter = created
        }
        let ratio = targetFormat.sampleRate / input.format.sampleRate
        let capacity = AVAudioFrameCount(
            (Double(input.frameLength) * ratio).rounded(.up)
        )
        guard let output = AVAudioPCMBuffer(
            pcmFormat: targetFormat,
            frameCapacity: capacity
        ) else {
            return nil
        }
        var conversionError: NSError?
        var suppliedInput = false
        let status = converter.convert(
            to: output,
            error: &conversionError
        ) { _, inputStatus in
            if suppliedInput {
                inputStatus.pointee = .noDataNow
                return nil
            }
            suppliedInput = true
            inputStatus.pointee = .haveData
            return input
        }
        guard status != .error, conversionError == nil else { return nil }
        return output
    }

    private func flushBufferedAudio() {
        guard !bufferedAudio.isEmpty else { return }
        onAudioChunk?(bufferedAudio)
        bufferedAudio.removeAll(keepingCapacity: true)
    }
}

extension SystemMeetingAudioCapture: SCStreamOutput, SCStreamDelegate {
    func stream(
        _ stream: SCStream,
        didOutputSampleBuffer sampleBuffer: CMSampleBuffer,
        of outputType: SCStreamOutputType
    ) {
        guard outputType == .audio else { return }
        process(sampleBuffer)
    }

    func stream(_ stream: SCStream, didStopWithError error: any Error) {
        onError?(error)
    }
}
