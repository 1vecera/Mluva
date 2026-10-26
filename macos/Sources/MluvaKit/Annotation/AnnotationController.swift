import Foundation
import Observation

/// Screen annotation while dictating: hold Shift to draw, Shift-click to pin. Marks become a
/// private bundle of keyframes whose summary follows the dictation.
@MainActor
@Observable
public final class AnnotationController {
    public private(set) var active = false
    public private(set) var status: String?
    private let paths: AppPaths

    public init(paths: AppPaths) {
        self.paths = paths
    }

    public func recordingChanged(_ recording: Bool, incognito: Bool) {
        active = recording && !incognito
    }

    /// The summary to append, or nil when nothing was marked.
    public func finish(transcript: TranscriptionResult) async -> String? {
        nil
    }
}
