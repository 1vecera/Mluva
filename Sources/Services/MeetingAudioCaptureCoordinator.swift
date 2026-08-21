import Foundation

protocol MeetingAudioCapturing: AnyObject {
    var onAudioChunk: ((Data) -> Void)? { get set }
    var onError: ((Error) -> Void)? { get set }

    func start() async throws
    func stop() async
}

final class MeetingAudioCaptureCoordinator: MeetingAudioCapturing, @unchecked Sendable {
    var onAudioChunk: ((Data) -> Void)?
    var onError: ((Error) -> Void)?

    private let microphone: any MeetingAudioSourceCapturing
    private let system: any MeetingAudioSourceCapturing
    private let processingQueue = DispatchQueue(
        label: "com.voicescribe.meeting-audio-mix",
        qos: .userInteractive
    )
    private let lock = NSLock()
    private var mixer: MeetingAudioMixer
    private var isStarted = false

    init(
        microphone: any MeetingAudioSourceCapturing = MicrophoneMeetingAudioCapture(),
        system: any MeetingAudioSourceCapturing = SystemMeetingAudioCapture(),
        chunkBytes: Int = 3_200
    ) {
        self.microphone = microphone
        self.system = system
        mixer = MeetingAudioMixer(chunkBytes: chunkBytes)
    }

    func start() async throws {
        guard !lock.withLock({ isStarted }) else { return }
        microphone.onAudioChunk = { [weak self] data in
            self?.enqueue(data, source: .microphone)
        }
        system.onAudioChunk = { [weak self] data in
            self?.enqueue(data, source: .system)
        }
        microphone.onError = { [weak self] error in self?.onError?(error) }
        system.onError = { [weak self] error in self?.onError?(error) }

        try await system.start()
        do {
            try await microphone.start()
            lock.withLock { isStarted = true }
        } catch {
            await system.stop()
            throw error
        }
    }

    func stop() async {
        let shouldStop = lock.withLock { () -> Bool in
            guard isStarted else { return false }
            isStarted = false
            return true
        }
        guard shouldStop else { return }
        await microphone.stop()
        await system.stop()
        await withCheckedContinuation { continuation in
            processingQueue.async { [weak self] in
                if let self {
                    for chunk in self.mixer.flush() {
                        self.onAudioChunk?(chunk)
                    }
                }
                continuation.resume()
            }
        }
    }

    private func enqueue(_ data: Data, source: MeetingAudioSource) {
        processingQueue.async { [weak self] in
            guard let self else { return }
            for chunk in mixer.append(data, from: source) {
                onAudioChunk?(chunk)
            }
        }
    }
}
