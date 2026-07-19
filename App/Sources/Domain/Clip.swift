import Foundation

// MARK: - Clip

/// Native presentation value projected from the shared clip core. Codable is
/// retained for legacy import and export, never as a second durable writer.
/// A user-authored excerpt of an episode — the foundation of the Snipd-style
/// share flow. Created from the transcript via the long-press composer
/// (UX-03 §6.4 / §6.6) or auto-captured from playback (auto-snip / lock-screen
/// / headphone path) and later rendered as audio + waveform card, video, or
/// deep link by the share-target stack.
///
/// `startMs` / `endMs` are sentence-snapped at composer-commit time so the
/// excerpt always lands on prose boundaries; the optional word-snap mode
/// belongs to v2 of the composer. `transcriptText` is captured *at creation
/// time* so the sharable surface can render even if the underlying transcript
/// is later re-ingested or relocated. `speakerID` is the transcript's stable
/// `Speaker.id.uuidString`, chosen by the composer when the clip falls inside
/// a single speaker's run. A pre-kernel import may retain its display label.
struct Clip: Codable, Sendable, Hashable, Identifiable {
    let id: UUID
    var revision: UInt64
    let episodeID: UUID
    let subscriptionID: UUID
    /// Sentence-snapped start, milliseconds from the episode origin.
    var startMs: Int
    /// Sentence-snapped end, milliseconds from the episode origin.
    var endMs: Int
    let createdAt: Date
    /// User-editable headline shown above the prose on rendered shares.
    var caption: String?
    /// Speaker handle when the clip falls inside one speaker's run. We store
    /// Newly authored clips use `Speaker.id.uuidString`; legacy imports may
    /// retain a display label that predates the typed speaker boundary.
    var speakerID: String?
    /// The captured prose, frozen at creation time. The transcript is the
    /// source of truth at the moment the user clipped — re-ingesting later
    /// must not silently rewrite a saved excerpt. Empty string when no
    /// transcript was available at capture (auto-snip without ingest).
    var transcriptText: String
    /// How the clip was triggered. `.touch` is the in-app composer path;
    /// `.auto` covers headphone / lock-screen / post-event auto capture.
    var source: Source
    var deleted: Bool
    var evidence: ClipEvidence?

    /// Origin of the clip capture. `.touch` is the in-app composer; the
    /// remaining cases describe auto-snip pathways introduced by the
    /// auto-snip / AI-chapters work.
    enum Source: String, Codable, Sendable, Hashable {
        case touch
        case auto
        case headphone
        case carplay
        case watch
        case siri
        case agent
        /// A future shared-core capture source this app version cannot name.
        case unsupported
    }

    init(
        id: UUID = UUID(),
        revision: UInt64 = 1,
        episodeID: UUID,
        subscriptionID: UUID,
        startMs: Int,
        endMs: Int,
        createdAt: Date = Date(),
        caption: String? = nil,
        speakerID: String? = nil,
        transcriptText: String = "",
        source: Source = .touch,
        deleted: Bool = false,
        evidence: ClipEvidence? = nil
    ) {
        self.id = id
        self.revision = revision
        self.episodeID = episodeID
        self.subscriptionID = subscriptionID
        self.startMs = startMs
        self.endMs = endMs
        self.createdAt = createdAt
        self.caption = caption
        self.speakerID = speakerID
        self.transcriptText = transcriptText
        self.source = source
        self.deleted = deleted
        self.evidence = evidence
    }

    private enum CodingKeys: String, CodingKey {
        case id, revision, episodeID, subscriptionID, startMs, endMs, createdAt
        case caption, speakerID, transcriptText, source, deleted, evidence
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decode(UUID.self, forKey: .id)
        revision = try c.decodeIfPresent(UInt64.self, forKey: .revision) ?? 1
        episodeID = try c.decode(UUID.self, forKey: .episodeID)
        subscriptionID = try c.decode(UUID.self, forKey: .subscriptionID)
        startMs = try c.decode(Int.self, forKey: .startMs)
        endMs = try c.decode(Int.self, forKey: .endMs)
        createdAt = try c.decodeIfPresent(Date.self, forKey: .createdAt) ?? Date()
        caption = try c.decodeIfPresent(String.self, forKey: .caption)
        speakerID = try c.decodeIfPresent(String.self, forKey: .speakerID)
        transcriptText = try c.decodeIfPresent(String.self, forKey: .transcriptText) ?? ""
        source = try c.decodeIfPresent(Source.self, forKey: .source) ?? .touch
        deleted = try c.decodeIfPresent(Bool.self, forKey: .deleted) ?? false
        evidence = try c.decodeIfPresent(ClipEvidence.self, forKey: .evidence)
    }

    /// Wall-clock duration of the clip in seconds.
    var duration: TimeInterval { Double(endMs - startMs) / 1000 }
}

struct ClipEvidence: Codable, Sendable, Hashable {
    let generationID: String
    let transcriptVersionID: String
    let transcriptContentDigest: String
    let spanID: String
}

extension Clip {
    /// Start time as seconds, convenient for `AVAsset` / `CMTime` math.
    var startSeconds: TimeInterval { TimeInterval(startMs) / 1000.0 }
    /// End time as seconds.
    var endSeconds: TimeInterval { TimeInterval(endMs) / 1000.0 }
    /// Span duration in seconds. Always non-negative. Mirrors `duration`
    /// but exposes a non-negative guarantee for the share-target stack.
    var durationSeconds: TimeInterval { max(0, endSeconds - startSeconds) }
}
