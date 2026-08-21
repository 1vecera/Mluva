import Testing
import Foundation
@testable import VoiceScribeMac

@Suite("Chunk Buffer")
struct ChunkBufferTests {

    @Test("Complete chunk is emitted immediately")
    func completeChunkEmitted() {
        let buffer = ChunkBuffer(chunkSize: 4)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.append(Data([1, 2, 3, 4]))

        #expect(chunks.count == 1)
        #expect(chunks[0] == Data([1, 2, 3, 4]))
        #expect(buffer.pendingBytes == 0)
    }

    @Test("Partial data stays buffered")
    func partialDataBuffered() {
        let buffer = ChunkBuffer(chunkSize: 4)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.append(Data([1, 2]))

        #expect(chunks.isEmpty)
        #expect(buffer.pendingBytes == 2)
    }

    @Test("Large data produces multiple chunks")
    func multipleChunks() {
        let buffer = ChunkBuffer(chunkSize: 3)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.append(Data([1, 2, 3, 4, 5, 6, 7]))

        #expect(chunks.count == 2)
        #expect(chunks[0] == Data([1, 2, 3]))
        #expect(chunks[1] == Data([4, 5, 6]))
        #expect(buffer.pendingBytes == 1)
    }

    @Test("Flush emits remaining bytes")
    func flushRemainder() {
        let buffer = ChunkBuffer(chunkSize: 4)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.append(Data([1, 2]))
        #expect(chunks.isEmpty)

        buffer.flush()

        #expect(chunks.count == 1)
        #expect(chunks[0] == Data([1, 2]))
        #expect(buffer.pendingBytes == 0)
    }

    @Test("Flush on empty buffer does nothing")
    func flushEmpty() {
        let buffer = ChunkBuffer(chunkSize: 4)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.flush()

        #expect(chunks.isEmpty)
    }

    @Test("Incremental appends across multiple calls")
    func incrementalAppends() {
        let buffer = ChunkBuffer(chunkSize: 4)
        var chunks: [Data] = []
        buffer.onChunk = { chunks.append($0) }

        buffer.append(Data([1]))
        buffer.append(Data([2]))
        buffer.append(Data([3]))
        #expect(chunks.isEmpty)

        buffer.append(Data([4, 5]))
        #expect(chunks.count == 1)
        #expect(chunks[0] == Data([1, 2, 3, 4]))
        #expect(buffer.pendingBytes == 1)
    }

    @Test("3200 bytes = 100ms @ 16kHz/16-bit/mono")
    func audioSpecChunkSize() {
        let sampleRate = 16000
        let bytesPerSample = 2
        let durationMs = 100
        let expected = sampleRate * bytesPerSample * durationMs / 1000
        #expect(expected == 3200)
    }
}
