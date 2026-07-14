import Foundation

// MARK: - ChunkableTranscript
//
// Adapter that lets `Transcript` (which has no `podcastID` and stores
// timestamps in seconds) satisfy the `TranscriptLike` / `TranscriptSegment`
// protocol pair `ChunkBuilder` requires.

struct ChunkableTranscript: TranscriptLike {

    typealias Segment = ChunkableSegment

    let transcript: Transcript
    let podcastID: UUID

    var episodeID: UUID { transcript.episodeID }
    var segments: [ChunkableSegment] {
        transcript.segments.map { ChunkableSegment(segment: $0) }
    }
}

struct ChunkableSegment: TranscriptSegment {
    let segment: Segment

    var text: String { segment.text }
    var startMS: Int { Int((segment.start * 1000).rounded()) }
    var endMS: Int { Int((segment.end * 1000).rounded()) }
    var speakerID: UUID? { segment.speakerID }
}
