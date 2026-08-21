import Foundation

/// Buffers arbitrary data and emits fixed-size chunks.
/// Used by AudioCaptureService to produce exactly 3200-byte (100ms) PCM chunks.
final class ChunkBuffer {
    private var buffer = Data()
    let chunkSize: Int
    var onChunk: ((Data) -> Void)?

    init(chunkSize: Int) {
        self.chunkSize = chunkSize
    }

    /// Append data and emit any complete chunks via onChunk callback.
    func append(_ data: Data) {
        buffer.append(data)
        while buffer.count >= chunkSize {
            let chunk = buffer.prefix(chunkSize)
            onChunk?(Data(chunk))
            buffer.removeFirst(chunkSize)
        }
    }

    /// Emit any remaining buffered data (may be smaller than chunkSize).
    func flush() {
        if !buffer.isEmpty {
            onChunk?(buffer)
            buffer.removeAll()
        }
    }

    /// Number of bytes waiting in the buffer (less than chunkSize).
    var pendingBytes: Int { buffer.count }
}
