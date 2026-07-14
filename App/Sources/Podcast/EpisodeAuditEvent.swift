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
        /// RAG embedding/indexing failed (non-fatal — transcript is still readable).
        static let transcriptIndexFailed: Kind = "transcript.index.failed"
        /// User-initiated retry from the Diagnostics sheet.
        static let transcriptRetryRequested: Kind = "transcript.retry"

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
            case .transcriptSkipped: return "Transcription skipped"
            case .transcriptAttempt: return "Transcription started"
            case .transcriptPublisherFetch: return "Publisher transcript fetch"
            case .transcriptPublisherFailed: return "Publisher transcript failed"
            case .transcriptReady: return "Transcript ready"
            case .transcriptFailed: return "Transcription failed"
            case .transcriptIndexFailed: return "Transcript indexing failed"
            case .transcriptRetryRequested: return "Retry requested"
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
            case .transcriptSkipped: return "minus.circle"
            case .transcriptAttempt: return "waveform.badge.magnifyingglass"
            case .transcriptPublisherFetch: return "doc.text.magnifyingglass"
            case .transcriptPublisherFailed: return "doc.text.below.ecg"
            case .transcriptReady: return "checkmark.bubble.fill"
            case .transcriptFailed: return "exclamationmark.bubble.fill"
            case .transcriptIndexFailed: return "magnifyingglass"
            case .transcriptRetryRequested: return "arrow.clockwise"
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
