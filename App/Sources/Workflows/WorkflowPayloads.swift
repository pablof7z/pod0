import Foundation
import Pod0Core

struct TranscriptJobPayload: Codable, Sendable, Equatable {
    let provider: STTProvider
    let modelID: String
    let audioURL: URL
    let audioVersion: String
    let userInitiated: Bool
}

struct ScheduledRunPayload: Codable, Sendable, Equatable {
    let taskID: UUID
    let scheduledFor: Date
    let prompt: String
    let modelID: String
    let intervalSeconds: TimeInterval
}

/// Decode-only shape for the one-shot legacy download migration.
enum LegacyDownloadIntentOrigin: String, Codable, Sendable, Equatable {
    case user
    case playback
    case autoDownload
}

/// Decode-only shape for retired Swift JobStore rows.
struct LegacyDownloadJobPayload: Codable, Sendable, Equatable {
    let origin: LegacyDownloadIntentOrigin
    let enclosureURL: URL
    let audioVersion: String
}

struct TranscriptWorkflowSnapshot: Sendable, Equatable {
    let episodeID: UUID
    let sourceRevision: String
    let contentDigest: String
    let selectionRevision: UInt64
}
