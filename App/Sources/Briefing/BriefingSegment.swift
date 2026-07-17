import Foundation

// MARK: - BriefingSegment

/// A single editorial unit inside a briefing — a titled passage with TTS
/// narration and zero or more original-audio quotes pulled from a source
/// episode. Maps directly to a row in the segment rail (UX-08 §3).
///
/// Segments are produced by the composer and persisted alongside the
/// stitched audio so the player can render the rail, attribution chips, and
/// auto-scrolling transcript without re-running the LLM.
struct BriefingSegment: Codable, Sendable, Hashable, Identifiable {
    /// Stable identifier (also used as the segment's `glassEffectID` namespace).
    var id: UUID

    /// Ordering inside the parent script. Zero-based; used as the "1." / "2."
    /// label in the chapter list (W3).
    var index: Int

    /// Editorial title shown in the rail and chapter list.
    var title: String

    /// The TTS-narrated body in plain text. Becomes the live transcript pane
    /// (W2) — sentences here render in *sourced* vs *summary* ink based on
    /// whether they have an attached `attribution`.
    var bodyText: String

    /// Sources cited by this segment. Drives the attribution chips and the
    /// "go to source" deep-link. Multiple sources possible per segment when a
    /// claim aggregates across episodes.
    var attributions: [BriefingAttribution]

    /// Optional original-audio quotes spliced *inside* the TTS narration.
    /// When present, the stitcher inserts each quote at its `insertAfterChar`
    /// offset within `bodyText`, ducking surrounding TTS as needed.
    var quotes: [BriefingQuote]

    /// Aggregate target duration in seconds for this segment (TTS + quotes).
    /// Composer fills this from the LLM's pacing estimate; player uses it to
    /// pre-allocate rail pill widths before the audio asset is ready.
    var targetSeconds: TimeInterval

    init(
        id: UUID = UUID(),
        index: Int,
        title: String,
        bodyText: String,
        attributions: [BriefingAttribution] = [],
        quotes: [BriefingQuote] = [],
        targetSeconds: TimeInterval = 60
    ) {
        self.id = id
        self.index = index
        self.title = title
        self.bodyText = bodyText
        self.attributions = attributions
        self.quotes = quotes
        self.targetSeconds = targetSeconds
    }
}

// MARK: - Attribution

/// A single cited source attached to a segment (or a sentence within one).
///
/// This is a bridge to the shared `AudioComposeAttribution` type (moved to
/// `App/Sources/Audio/` since `AgentTTSComposer` — a kept feature — builds
/// on the same track/attribution shape). Kept as a typealias here so the
/// rest of the briefing pipeline can keep using the `BriefingAttribution`
/// name unchanged until the whole Briefing feature is deleted.
typealias BriefingAttribution = AudioComposeAttribution

/// Bridges to the shared `AudioComposeTrack`/`ComposedAudioStitcher` types
/// (moved to `App/Sources/Audio/` for `AgentTTSComposer`). See
/// `BriefingAttribution` above for the same rationale.
typealias BriefingTrack = AudioComposeTrack
typealias BriefingAudioStitcher = ComposedAudioStitcher

// MARK: - Quote

/// A spliced original-audio excerpt from a source episode. The stitcher pulls
/// audio from `mediaURL` between `[startSeconds, endSeconds]` and inserts it
/// after `insertAfterChar` characters of `BriefingSegment.bodyText`.
struct BriefingQuote: Codable, Sendable, Hashable, Identifiable {
    var id: UUID
    /// FK into `Episode.id` — needed to resolve `mediaURL` lazily so the
    /// composer doesn't have to capture full `Episode` snapshots.
    var episodeID: UUID
    /// Direct URL to the source episode's enclosure (alias for the project's
    /// `Episode.mediaURL`). The lane spec uses `enclosureURL`; we honor that
    /// terminology in the data type even though the runtime model resolves
    /// from `mediaURL`.
    var enclosureURL: URL
    var startSeconds: TimeInterval
    var endSeconds: TimeInterval
    /// Character offset inside the parent segment's `bodyText` after which
    /// this quote should play. `0` plays *before* the TTS narration.
    var insertAfterChar: Int
    /// The text of the excerpt, used for the live transcript and *paraphrased*
    /// fallback when the audio fetch fails.
    var transcriptText: String

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        enclosureURL: URL,
        startSeconds: TimeInterval,
        endSeconds: TimeInterval,
        insertAfterChar: Int = 0,
        transcriptText: String
    ) {
        self.id = id
        self.episodeID = episodeID
        self.enclosureURL = enclosureURL
        self.startSeconds = startSeconds
        self.endSeconds = endSeconds
        self.insertAfterChar = insertAfterChar
        self.transcriptText = transcriptText
    }

    var durationSeconds: TimeInterval {
        max(0, endSeconds - startSeconds)
    }
}
