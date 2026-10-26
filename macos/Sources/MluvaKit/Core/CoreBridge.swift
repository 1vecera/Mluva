import CMluvaFFI
import Foundation
import os

public struct BridgeError: Error, LocalizedError, Equatable, Sendable {
    public let message: String
    public init(_ message: String) { self.message = message }
    public var errorDescription: String? { message }
}

/// Events the shared core sends while work runs: realtime previews and agent send outcomes.
public enum CoreEvent: Sendable, Equatable {
    case realtimePreview(session: String, committed: String, volatile: String)
    case route(id: String, outcome: String)
}

/// The in-process shared Rust core. Calls are synchronous JSON requests; long work returns a
/// task identifier whose result arrives as an event and resumes the awaiting caller.
public final class CoreBridge: @unchecked Sendable {
    public static let shared = CoreBridge()

    private struct Pending {
        var waiting: [String: CheckedContinuation<JSONValue, any Error>] = [:]
        /// Results that arrived before their caller started waiting.
        var early: [String: Result<JSONValue, BridgeError>] = [:]
        var listeners: [UUID: @Sendable (CoreEvent) -> Void] = [:]
    }

    private let state = OSAllocatedUnfairLock(initialState: Pending())

    private init() {
        let context = Unmanaged.passUnretained(self).toOpaque()
        mluva_set_event_handler({ context, event in
            guard let context, let event else { return }
            let bridge = Unmanaged<CoreBridge>.fromOpaque(context).takeUnretainedValue()
            bridge.receive(Data(String(cString: event).utf8))
        }, context)
    }

    /// Run one request and return its `ok` value.
    @discardableResult
    public func call(_ method: String, _ params: JSONValue = [:]) throws -> JSONValue {
        let request: JSONValue = ["method": .string(method), "params": params]
        let text = String(decoding: try JSONEncoder().encode(request), as: UTF8.self)
        guard let raw = mluva_call(text) else { throw BridgeError("The core returned nothing.") }
        defer { mluva_string_free(raw) }
        let response = try JSONDecoder().decode(JSONValue.self, from: Data(String(cString: raw).utf8))
        if let error = response["error"]?.string { throw BridgeError(error) }
        return response["ok"] ?? .null
    }

    public func call<T: Decodable>(_ method: String, _ params: JSONValue = [:], as type: T.Type) throws -> T {
        try call(method, params).decode(T.self)
    }

    /// Start background work and wait for its result.
    public func task(_ method: String, _ params: JSONValue = [:]) async throws -> JSONValue {
        let started = try call(method, params)
        guard let task = started["task"]?.string else { throw BridgeError("The core did not start `\(method)`.") }
        return try await withCheckedThrowingContinuation { continuation in
            let early: Result<JSONValue, BridgeError>? = state.withLock { pending in
                if let result = pending.early.removeValue(forKey: task) { return result }
                pending.waiting[task] = continuation
                return nil
            }
            if let early { continuation.resume(with: early.mapError { $0 as any Error }) }
        }
    }

    public func task<T: Decodable>(_ method: String, _ params: JSONValue = [:], as type: T.Type) async throws -> T {
        try await task(method, params).decode(T.self)
    }

    /// Listen for events until the returned token is cancelled.
    public func listen(_ listener: @escaping @Sendable (CoreEvent) -> Void) -> ListenerToken {
        let id = UUID()
        state.withLock { $0.listeners[id] = listener }
        return ListenerToken { [weak self] in self?.state.withLock { _ = $0.listeners.removeValue(forKey: id) } }
    }

    /// Queue PCM for a realtime session; false once the session cannot take more audio.
    public func realtimeAudio(session: String, pcm: Data) -> Bool {
        pcm.withUnsafeBytes { buffer in
            mluva_realtime_audio(session, buffer.bindMemory(to: UInt8.self).baseAddress, buffer.count)
        }
    }

    private func receive(_ data: Data) {
        guard let event = try? JSONDecoder().decode(JSONValue.self, from: data) else { return }
        switch event["type"]?.string {
        case "task":
            guard let task = event["task"]?.string else { return }
            let result: Result<JSONValue, BridgeError> =
                event["error"]?.string.map { .failure(BridgeError($0)) } ?? .success(event["ok"] ?? .null)
            let continuation = state.withLock { pending in
                if let waiting = pending.waiting.removeValue(forKey: task) { return Optional(waiting) }
                pending.early[task] = result
                return nil
            }
            continuation?.resume(with: result.mapError { $0 as any Error })
        case "realtime.preview":
            broadcast(.realtimePreview(
                session: event["session"]?.string ?? "",
                committed: event["committed"]?.string ?? "",
                volatile: event["volatile"]?.string ?? ""))
        case "route":
            broadcast(.route(id: event["id"]?.string ?? "", outcome: event["outcome"]?.string ?? ""))
        default:
            break
        }
    }

    private func broadcast(_ event: CoreEvent) {
        let listeners = state.withLock { Array($0.listeners.values) }
        for listener in listeners { listener(event) }
    }
}

public final class ListenerToken: Sendable {
    private let cancelAction: @Sendable () -> Void
    init(_ cancel: @escaping @Sendable () -> Void) { cancelAction = cancel }
    public func cancel() { cancelAction() }
    deinit { cancelAction() }
}
