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

struct NotificationJobPayload: Codable, Sendable, Equatable {
    let discoveredAt: Date
    let podcastID: UUID
    let episodeTitle: String
}

struct AutoDownloadJobPayload: Codable, Sendable, Equatable {
    let discoveryOccurrenceID: String
    let policyVersion: String
}

enum DownloadIntentOrigin: String, Codable, Sendable, Equatable {
    case user
    case playback
    case autoDownload

    var priority: Int {
        switch self {
        case .user: 100
        case .playback: 80
        case .autoDownload: 20
        }
    }
}

struct DownloadJobPayload: Codable, Sendable, Equatable {
    let origin: DownloadIntentOrigin
    let enclosureURL: URL
    let audioVersion: String
}

struct FeedDiscoveryPayload: Codable, Sendable, Equatable {
    struct EpisodeInput: Codable, Sendable, Equatable {
        let episodeID: UUID
        let inputVersion: String
        let pubDate: Date
        let title: String
    }

    let podcastID: UUID
    let occurrenceID: String
    let discoveredAt: Date
    let episodes: [EpisodeInput]
    let autoDownloadPolicy: AutoDownloadPolicy?
    let notificationsEnabled: Bool
    let policyVersion: String
}

struct TranscriptWorkflowSnapshot: Sendable, Equatable {
    let episodeID: UUID
    let sourceRevision: String
    let contentDigest: String
    let selectionRevision: UInt64
}
