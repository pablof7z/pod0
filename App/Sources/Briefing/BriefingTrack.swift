import Foundation

// MARK: - BriefingTrack

/// One playable unit emitted by the composer for the player engine to step
/// through sequentially. A `BriefingTrack` is the bridge between the editorial
/// `BriefingSegment` model and the AVFoundation timeline — it points at a
/// concrete audio file (or a slice of one) along with the metadata the rail
/// and transcript pane need at playback time.
///
/// One segment may produce multiple tracks (TTS pre-quote, original-audio
/// quote, TTS post-quote) so the player can emit accurate "this is a quote"
/// chrome state changes mid-segment without re-parsing the segment body.
struct BriefingTrack: Sendable, Hashable, Identifiable {
    /// Stable identifier — distinct from the source segment's id so multi-track
    /// segments don't collide in `glassEffectID` namespacing.
    var id: UUID

    /// FK back to the producing `BriefingSegment.id`. The player groups tracks
    /// by this id when rendering the rail.
    var segmentID: UUID

    /// Ordering inside the parent segment.
    var indexInSegment: Int

    /// What kind of source this track plays. Drives chrome state — a `.quote`
    /// track shows the attribution chip prominently and an audio-waveform glyph
    /// on the rail; `.tts` tracks render the live-transcript serif body.
    var kind: Kind

    /// On-disk URL the player should hand to AVFoundation. For `.tts` tracks
    /// this is the synthesized m4a; for `.quote` tracks it points at the
    /// excerpted, time-trimmed copy of the source enclosure (or a paraphrase
    /// fallback when the original fetch failed).
    var audioURL: URL

    /// Intra-track time range used by the stitcher when reconstructing the
    /// full briefing waveform. The player's scrubber uses cumulative durations.
    var startInTrackSeconds: TimeInterval
    var endInTrackSeconds: TimeInterval

    /// Plain-text caption shown in the live transcript while this track plays.
    /// For `.tts` it's the corresponding slice of `bodyText`; for `.quote` it's
    /// the quote's `transcriptText`.
    var transcriptText: String

    /// Optional attribution surfaced while this track plays. Only set for
    /// `.quote` tracks (so episode chrome lights up) and for sentences within
    /// `.tts` tracks the LLM marked as *sourced*.
    var attribution: BriefingAttribution?

    /// `true` for tracks that substitute paraphrased TTS for a failed
    /// original-audio fetch. Drives the *paraphrased* chip (UX-08 §7).
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
        attribution: BriefingAttribution? = nil,
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
