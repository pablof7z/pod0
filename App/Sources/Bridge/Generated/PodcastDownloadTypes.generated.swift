// PodcastDownloadTypes.generated.swift
// Hand-maintained mirror of Rust projection types. See PodcastUpdate.generated.swift.

import Foundation

/// Active download-queue projection surfaced via `PodcastUpdate.downloads`.
struct DownloadQueueSnapshot: Equatable {
    var active: [DownloadItemSnapshot] = []
    var queuedCount: Int = 0
    var completedToday: Int = 0
}

/// One row in `DownloadQueueSnapshot.active`.
struct DownloadItemSnapshot: Identifiable, Equatable {
    var episodeId: String
    /// What this row fetches. Omitted on the wire for episodes (the default),
    /// so it must decode-default to `.episode`. Lets the model UI pick out its
    /// own rows and lets the episode overlay skip non-episode rows.
    var kind: DownloadKind = .episode
    var progress: Double = 0
    var state: String
    /// Total file size (bytes) once the server reports `Content-Length`.
    /// `nil` until the first HTTP response arrives.
    var totalBytes: Int64? = nil
    var error: String? = nil

    var id: String { episodeId }
}

extension DownloadQueueSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        active = try c.decodeIfPresent([DownloadItemSnapshot].self, forKey: .active) ?? []
        queuedCount = try c.decodeIfPresent(Int.self, forKey: .queuedCount) ?? 0
        completedToday = try c.decodeIfPresent(Int.self, forKey: .completedToday) ?? 0
    }
}

extension DownloadItemSnapshot: Codable {
    init(from decoder: Decoder) throws {
        let c = try decoder.container(keyedBy: CodingKeys.self)
        episodeId = try c.decode(String.self, forKey: .episodeId)
        kind = try c.decodeIfPresent(DownloadKind.self, forKey: .kind) ?? .episode
        progress = try c.decodeIfPresent(Double.self, forKey: .progress) ?? 0
        state = try c.decode(String.self, forKey: .state)
        totalBytes = try c.decodeIfPresent(Int64.self, forKey: .totalBytes)
        error = try c.decodeIfPresent(String.self, forKey: .error)
    }
}
