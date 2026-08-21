import Foundation

protocol TranscriptionProvider: AnyObject {
    var kind: TranscriptionProviderKind { get }
    var onEvent: ((TranscriptEvent) -> Void)? { get set }

    func start() async throws
    func appendAudio(_ data: Data)
    func finish() async throws -> String
    func cancel()
}
