import Foundation

// MARK: - EpisodeAuditEvent

/// One entry in an episode's diagnostic audit log.
///
/// The log answers user-facing questions like "why don't most episodes have a
/// transcript?" — so the event model is deliberately verbose: every decision
/// the pipeline makes (including silent skips) becomes an event so the
/// Diagnostics sheet can show *exactly* what happened and what the system
/// decided not to attempt.
///
/// Persistence: forward-compat decoding via `decodeIfPresent`. The
/// `kind` discriminator is a free-form string rather than an enum so adding
/// new event types in a later release doesn't make older logs un-readable.
struct EpisodeAuditEvent: Codable, Sendable, Hashable, Identifiable {
    var id: UUID
    var episodeID: UUID
    var timestamp: Date
    var kind: Kind
    var severity: Severity
    /// One-line summary suitable for the row title.
    var summary: String
    /// Optional structured detail key/value pairs surfaced in the expanded row.
    /// Stored as an ordered array (not a dict) so the UI can present fields in
    /// a stable, meaningful order — URL first, then HTTP status, etc.
    var details: [Detail]

    init(
        id: UUID = UUID(),
        episodeID: UUID,
        timestamp: Date = Date(),
        kind: Kind,
        severity: Severity,
        summary: String,
        details: [Detail] = []
    ) {
        self.id = id
        self.episodeID = episodeID
        self.timestamp = timestamp
        self.kind = kind
        self.severity = severity
        self.summary = summary
        self.details = details
    }

    struct Detail: Codable, Sendable, Hashable {
        var label: String
        var value: String

        init(_ label: String, _ value: String) {
            self.label = label
            self.value = value
        }
    }

    enum Severity: String, Codable, Sendable, Hashable {
        case info
        case success
        case warning
        case failure
    }

    /// Free-form discriminator. Use the constants in `Kind.Constants` for
    /// recording known events; unknown values decode without error so older
    /// logs survive a release that retired an event type.
    struct Kind: Codable, Sendable, Hashable, RawRepresentable, ExpressibleByStringLiteral {
        let rawValue: String
        init(rawValue: String) { self.rawValue = rawValue }
        init(stringLiteral value: String) { self.rawValue = value }

        // MARK: Download lifecycle
        static let downloadRequested: Kind = "download.requested"
        static let downloadStarted: Kind = "download.started"
        static let downloadFinished: Kind = "download.finished"
        static let downloadFailed: Kind = "download.failed"
        static let downloadCancelled: Kind = "download.cancelled"
        static let downloadDeleted: Kind = "download.deleted"
        static let downloadDeleteFailed: Kind = "download.delete_failed"

        // MARK: Transcript lifecycle
        /// We chose not to attempt transcription. `details` carries the reason.
        static let transcriptSkipped: Kind = "transcript.skipped"
        /// Beginning of an actual transcription attempt (after gating).
        static let transcriptAttempt: Kind = "transcript.attempt"
        /// Publisher `<podcast:transcript>` fetch attempted.
        static let transcriptPublisherFetch: Kind = "transcript.publisher.fetch"
        /// Publisher fetch failed; pipeline will (or won't) fall through to STT.
        static let transcriptPublisherFailed: Kind = "transcript.publisher.failed"
        /// Final terminal success — transcript persisted and `.ready`.
        static let transcriptReady: Kind = "transcript.ready"
        /// Terminal failure for the whole pipeline.
        static let transcriptFailed: Kind = "transcript.failed"
        /// Transcript chunks embedded + upserted into the RAG search index.
        static let transcriptIndexed: Kind = "transcript.indexed"
        /// RAG embedding/indexing failed (non-fatal — transcript is still readable).
        static let transcriptIndexFailed: Kind = "transcript.index.failed"
        /// User-initiated retry from the Diagnostics sheet.
        static let transcriptRetryRequested: Kind = "transcript.retry"

        // MARK: Identification (chapters + ads)
        static let chaptersAttempt: Kind = "chapters.attempt"
        static let chaptersReady: Kind = "chapters.ready"
        static let chaptersFailed: Kind = "chapters.failed"
        static let adsReady: Kind = "ads.ready"

        // MARK: Playback lifecycle
        static let playbackStarted: Kind = "playback.started"
        static let playbackCompleted: Kind = "playback.completed"

        // MARK: Clipping lifecycle
        static let clipCreated: Kind = "clip.created"
        static let clipExported: Kind = "clip.exported"
        static let clipShared: Kind = "clip.shared"
        static let clipFailed: Kind = "clip.failed"

        // MARK: Auto-download policy
        static let autoDownloadQueued: Kind = "auto_download.queued"
        static let autoDownloadDeferred: Kind = "auto_download.deferred"

        /// Human-friendly label for UI rendering. The view falls back to
        /// `rawValue` when the kind is unrecognised so old logs still render.
        var displayLabel: String {
            switch self {
            case .downloadRequested: return "Download requested"
            case .downloadStarted: return "Download started"
            case .downloadFinished: return "Download finished"
            case .downloadFailed: return "Download failed"
            case .downloadCancelled: return "Download cancelled"
            case .downloadDeleted: return "Download deleted"
            case .downloadDeleteFailed: return "Download delete failed"
            case .transcriptSkipped: return "Transcription skipped"
            case .transcriptAttempt: return "Transcription started"
            case .transcriptPublisherFetch: return "Publisher transcript fetch"
            case .transcriptPublisherFailed: return "Publisher transcript failed"
            case .transcriptReady: return "Transcript ready"
            case .transcriptFailed: return "Transcription failed"
            case .transcriptIndexed: return "Indexed for search"
            case .transcriptIndexFailed: return "Search indexing failed"
            case .transcriptRetryRequested: return "Retry requested"
            case .chaptersAttempt: return "Chapter identification started"
            case .chaptersReady: return "Chapters identified"
            case .chaptersFailed: return "Chapter identification failed"
            case .adsReady: return "Ad segments identified"
            case .playbackStarted: return "Playback started"
            case .playbackCompleted: return "Playback completed"
            case .clipCreated: return "Clip created"
            case .clipExported: return "Clip exported"
            case .clipShared: return "Clip shared"
            case .clipFailed: return "Clip export failed"
            case .autoDownloadQueued: return "Auto-download queued"
            case .autoDownloadDeferred: return "Auto-download deferred"
            default: return rawValue
            }
        }

        /// SF Symbol for the row leading icon.
        var iconName: String {
            switch self {
            case .downloadRequested: return "arrow.down.circle"
            case .downloadStarted: return "arrow.down.circle.fill"
            case .downloadFinished: return "checkmark.circle.fill"
            case .downloadFailed: return "exclamationmark.triangle.fill"
            case .downloadCancelled: return "xmark.circle"
            case .downloadDeleted: return "trash"
            case .downloadDeleteFailed: return "trash.slash"
            case .transcriptSkipped: return "minus.circle"
            case .transcriptAttempt: return "waveform.badge.magnifyingglass"
            case .transcriptPublisherFetch: return "doc.text.magnifyingglass"
            case .transcriptPublisherFailed: return "doc.text.below.ecg"
            case .transcriptReady: return "checkmark.bubble.fill"
            case .transcriptFailed: return "exclamationmark.bubble.fill"
            case .transcriptIndexed: return "sparkle.magnifyingglass"
            case .transcriptIndexFailed: return "magnifyingglass"
            case .transcriptRetryRequested: return "arrow.clockwise"
            case .chaptersAttempt: return "list.bullet.rectangle"
            case .chaptersReady: return "list.bullet.rectangle.fill"
            case .chaptersFailed: return "list.bullet.rectangle.portrait"
            case .adsReady: return "dollarsign.circle.fill"
            case .playbackStarted: return "play.circle"
            case .playbackCompleted: return "checkmark.seal.fill"
            case .clipCreated: return "scissors"
            case .clipExported: return "square.and.arrow.up"
            case .clipShared: return "square.and.arrow.up.fill"
            case .clipFailed: return "scissors.badge.ellipsis"
            case .autoDownloadQueued: return "arrow.down.to.line.circle"
            case .autoDownloadDeferred: return "pause.circle"
            default: return "circle"
            }
        }
    }

    // MARK: - Codable (forward-compat)

    private enum CodingKeys: String, CodingKey {
        case id, episodeID, timestamp, kind, severity, summary, details
    }

    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        id = try c.decodeIfPresent(UUID.self, forKey: .id) ?? UUID()
        episodeID = try c.decode(UUID.self, forKey: .episodeID)
        timestamp = try c.decodeIfPresent(Date.self, forKey: .timestamp)
            ?? Date(timeIntervalSince1970: 0)
        let rawKind = try c.decodeIfPresent(String.self, forKey: .kind) ?? ""
        kind = Kind(rawValue: rawKind)
        let rawSeverity = try c.decodeIfPresent(String.self, forKey: .severity) ?? "info"
        severity = Severity(rawValue: rawSeverity) ?? .info
        summary = try c.decodeIfPresent(String.self, forKey: .summary) ?? ""
        details = try c.decodeIfPresent([Detail].self, forKey: .details) ?? []
    }

    func encode(to encoder: Encoder) throws {
        var c = encoder.container(keyedBy: CodingKeys.self)
        try c.encode(id, forKey: .id)
        try c.encode(episodeID, forKey: .episodeID)
        try c.encode(timestamp, forKey: .timestamp)
        try c.encode(kind.rawValue, forKey: .kind)
        try c.encode(severity.rawValue, forKey: .severity)
        try c.encode(summary, forKey: .summary)
        try c.encode(details, forKey: .details)
    }
}
