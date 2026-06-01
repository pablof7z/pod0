import Foundation

// MARK: - NarrationTrack

/// One playable unit handed to `NarrationAudioStitcher` to be concatenated into
/// a single audio file. A `NarrationTrack` points at a concrete audio file (or
/// a time-slice of one) plus the metadata the chapter/transcript builder needs.
///
/// Used by `AgentTTSComposer` to assemble agent-generated episodes from a mix
/// of synthesized speech (`.tts`) and trimmed source-episode clips (`.quote`).
struct NarrationTrack: Sendable, Hashable, Identifiable {
    /// Stable identifier, distinct from the parent grouping id.
    var id: UUID

    /// FK back to the producing group (one logical segment may emit several
    /// tracks). Callers that don't group use a single shared id.
    var segmentID: UUID

    /// Ordering inside the parent group.
    var indexInSegment: Int

    /// What kind of source this track plays.
    var kind: Kind

    /// On-disk URL the stitcher hands to AVFoundation. For `.tts` tracks this
    /// is the synthesized audio; for `.quote` tracks it points at the source
    /// enclosure that gets time-trimmed during stitching.
    var audioURL: URL

    /// Intra-track time range the stitcher uses when reconstructing the full
    /// waveform.
    var startInTrackSeconds: TimeInterval
    var endInTrackSeconds: TimeInterval

    /// Plain-text caption for the corresponding transcript segment.
    var transcriptText: String

    init(
        id: UUID = UUID(),
        segmentID: UUID,
        indexInSegment: Int,
        kind: Kind,
        audioURL: URL,
        startInTrackSeconds: TimeInterval = 0,
        endInTrackSeconds: TimeInterval,
        transcriptText: String
    ) {
        self.id = id
        self.segmentID = segmentID
        self.indexInSegment = indexInSegment
        self.kind = kind
        self.audioURL = audioURL
        self.startInTrackSeconds = startInTrackSeconds
        self.endInTrackSeconds = endInTrackSeconds
        self.transcriptText = transcriptText
    }

    var durationSeconds: TimeInterval {
        max(0, endInTrackSeconds - startInTrackSeconds)
    }

    enum Kind: String, Codable, Sendable, Hashable {
        case tts
        case quote
    }
}
