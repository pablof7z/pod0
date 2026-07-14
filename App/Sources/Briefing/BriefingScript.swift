import Foundation

// MARK: - BriefingScript

/// The persisted, fully-composed product of one `BriefingRequest`.
///
/// Pure data — no AVFoundation, no SwiftUI. The script is what the composer
/// hands to the player and what `BriefingStorage` writes to disk as
/// `<id>.json`. The corresponding stitched audio lives at `<id>.m4a`.
///
/// A script is *complete* when every segment has either a TTS audio URL,
/// a quote, or a paraphrased fallback. Partials remain valid artifacts (UX-08
/// §7 — *Mid-generation cancel*) and are flagged via `isPartial`.
struct BriefingScript: Codable, Sendable, Hashable, Identifiable {
    /// Same UUID as the originating request — one script per request, ever.
    var id: UUID

    /// Editorial title displayed in the player and library row.
    /// Example: *"Tuesday Briefing"*.
    var title: String

    /// One-line subtitle. Example: *"8 min · drawn from 7 episodes"*.
    var subtitle: String

    /// Echo of the originating request for replay / regeneration / debugging.
    var request: BriefingRequest

    /// Ordered segments. The intro/outro stings are themselves segments
    /// (with a recognisable `kind` chip in the rail) so the player only ever
    /// iterates one list.
    var segments: [BriefingSegment]

    /// All sources cited at least once across `segments`. Surfaced in the
    /// saved-detail view (W5) as the *Sources* list.
    var sources: [BriefingAttribution]

    /// Branches that were *recorded* during a previous playback — restored
    /// from disk so they reappear as side-paths on re-listen (UX-08 §3).
    var recordedBranches: [BriefingBranch]

    /// When the composer finished. Distinct from `request.requestedAt` to
    /// surface generation latency in debug surfaces.
    var generatedAt: Date

    /// Total stitched duration in seconds of all segments. Authoritative —
    /// the player uses it for the scrubber max, not `BriefingLength.targetSeconds`.
    var totalDurationSeconds: TimeInterval

    /// `true` when generation was cancelled mid-flight; the script may have
    /// fewer segments than the prompt requested.
    var isPartial: Bool

    init(
        id: UUID,
        title: String,
        subtitle: String,
        request: BriefingRequest,
        segments: [BriefingSegment],
        sources: [BriefingAttribution] = [],
        recordedBranches: [BriefingBranch] = [],
        generatedAt: Date = Date(),
        totalDurationSeconds: TimeInterval,
        isPartial: Bool = false
    ) {
        self.id = id
        self.title = title
        self.subtitle = subtitle
        self.request = request
        self.segments = segments
        self.sources = sources
        self.recordedBranches = recordedBranches
        self.generatedAt = generatedAt
        self.totalDurationSeconds = totalDurationSeconds
        self.isPartial = isPartial
    }
}

// MARK: - Branch

/// A recorded branch — the user said *"tell me more"*, the agent answered, the
/// briefing resumed. Persisted so the same fork resurfaces as an optional
/// side-path in the segment rail on subsequent listens.
struct BriefingBranch: Codable, Sendable, Hashable, Identifiable {
    var id: UUID
    /// Which segment was active when the branch fired.
    var parentSegmentID: UUID
    /// Sample-accurate position inside the parent at which the main thread
    /// froze. The contract is *pause-and-resume*, not *fork-and-replace*.
    var pausedAtSeconds: TimeInterval
    /// The user's verbatim question (or typed prompt).
    var prompt: String
    /// The agent's answer in plain text. Audio for the branch is *not*
    /// pre-rendered — branches are voice-mode replies, generated on demand.
    var answerText: String
    /// When the branch was recorded.
    var occurredAt: Date

    init(
        id: UUID = UUID(),
        parentSegmentID: UUID,
        pausedAtSeconds: TimeInterval,
        prompt: String,
        answerText: String,
        occurredAt: Date = Date()
    ) {
        self.id = id
        self.parentSegmentID = parentSegmentID
        self.pausedAtSeconds = pausedAtSeconds
        self.prompt = prompt
        self.answerText = answerText
        self.occurredAt = occurredAt
    }
}
