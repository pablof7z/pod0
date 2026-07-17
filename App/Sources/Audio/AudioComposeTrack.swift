import Foundation

// MARK: - AudioComposeTrack

/// One playable unit for `ComposedAudioStitcher` to step through
/// sequentially when assembling a multi-source audio file (TTS narration,
/// an original-audio clip, a sting, etc).
///
/// A single logical composition may produce multiple tracks (TTS
/// pre-quote, original-audio quote, TTS post-quote) so a player can emit
/// accurate "this is a quote" chrome state changes mid-composition without
/// re-parsing the source content.
struct AudioComposeTrack: Sendable, Hashable, Identifiable {
    /// Stable identifier — distinct from the source segment's id so multi-track
    /// segments don't collide in `glassEffectID` namespacing.
    var id: UUID

    /// FK back to the producing segment's id. Consumers group tracks by this
    /// id when rendering a rail.
    var segmentID: UUID

    /// Ordering inside the parent segment.
    var indexInSegment: Int

    /// What kind of source this track plays.
    var kind: Kind

    /// On-disk URL the player should hand to AVFoundation. For `.tts` tracks
    /// this is the synthesized m4a; for `.quote` tracks it points at the
    /// excerpted, time-trimmed copy of the source enclosure (or a paraphrase
    /// fallback when the original fetch failed).
    var audioURL: URL

    /// Intra-track time range used by the stitcher when reconstructing the
    /// full composed waveform. A scrubber uses cumulative durations.
    var startInTrackSeconds: TimeInterval
    var endInTrackSeconds: TimeInterval

    /// Plain-text caption shown in a live transcript while this track plays.
    var transcriptText: String

    /// Optional attribution surfaced while this track plays.
    var attribution: AudioComposeAttribution?

    /// `true` for tracks that substitute paraphrased TTS for a failed
    /// original-audio fetch.
    var isParaphrasedFallback: Bool

    init(
        id: UUID = UUID(),
        segmentID: UUID,
        indexInSegment: Int,
        kind: Kind,
        audioURL: URL,
        startInTrackSeconds: TimeInterval = 0,
        endInTrackSeconds: TimeInterval,
        transcriptText: String,
        attribution: AudioComposeAttribution? = nil,
        isParaphrasedFallback: Bool = false
    ) {
        self.id = id
        self.segmentID = segmentID
        self.indexInSegment = indexInSegment
        self.kind = kind
        self.audioURL = audioURL
        self.startInTrackSeconds = startInTrackSeconds
        self.endInTrackSeconds = endInTrackSeconds
        self.transcriptText = transcriptText
        self.attribution = attribution
        self.isParaphrasedFallback = isParaphrasedFallback
    }

    var durationSeconds: TimeInterval {
        max(0, endInTrackSeconds - startInTrackSeconds)
    }

    enum Kind: String, Codable, Sendable, Hashable {
        case tts
        case quote
        case sting   // intro / outro cinematic
    }
}

// MARK: - AudioComposeAttribution

/// Citation metadata attached to an `AudioComposeTrack`.
struct AudioComposeAttribution: Codable, Sendable, Hashable, Identifiable {
    var id: UUID
    /// Foreign key to the source episode (matches `Episode.id`). Optional so
    /// non-episode sources can also appear.
    var episodeID: UUID?
    /// Foreign key to a source knowledge page, if any.
    var wikiPageID: UUID?
    /// Human display label — e.g. *"Hard Fork · 34:12"*.
    var displayLabel: String
    /// Timestamp inside the source episode the citation jumps to. Optional
    /// for non-episode sources.
    var timestampSeconds: TimeInterval?

    init(
        id: UUID = UUID(),
        episodeID: UUID? = nil,
        wikiPageID: UUID? = nil,
        displayLabel: String,
        timestampSeconds: TimeInterval? = nil
    ) {
        self.id = id
        self.episodeID = episodeID
        self.wikiPageID = wikiPageID
        self.displayLabel = displayLabel
        self.timestampSeconds = timestampSeconds
    }
}
