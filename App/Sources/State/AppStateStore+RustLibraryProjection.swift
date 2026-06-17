import Foundation

// MARK: - Rust-owned library projection helpers
//
// Convenience wrappers for non-view call sites. They decode Rust-owned library
// facts and keep agent/service adapters from scanning Swift episode arrays.

extension AppStateStore {

    func rustEpisodeCount(forPodcast podcastID: UUID) -> Int {
        LibraryPodcastStatsProjection
            .load(podcastIDs: [podcastID], store: self)
            .episodeCount(for: podcastID)
    }

    func rustLatestEpisode(forPodcast podcastID: UUID) -> Episode? {
        LibraryPodcastStatsProjection
            .load(podcastIDs: [podcastID], store: self)
            .latestEpisode(for: podcastID, store: self)
    }

    func rustEpisodes(forPodcast podcastID: UUID, limit: Int = 10_000) -> [Episode] {
        guard let envelope = kernel?.libraryShowEpisodesEnvelope(podcastID: podcastID, limit: limit),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                ShowEpisodesResponse.self,
                from: data
              )
        else { return [] }
        return decoded.episodeIds.compactMap { episode(id: $0) }
    }

    func rustInProgressEpisodes(limit: Int = 30) -> [Episode] {
        rustListenNow(limit: limit).inProgressEpisodes
    }

    func rustRecentEpisodes(limit: Int = 30) -> [Episode] {
        rustListenNow(limit: limit).latestEpisodes
    }

    func rustEpisodeIDForAudioURL(_ audioURLString: String, podcastID: UUID) -> UUID? {
        guard let envelope = kernel?.libraryEpisodeForAudioURLEnvelope(
            audioURL: audioURLString,
            podcastID: podcastID
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                EpisodeForAudioURLResponse.self,
                from: data
              )
        else { return nil }
        return decoded.episodeId
    }

    func rustTotalUnplayedCount() -> Int {
        rustLibrarySummary()?.totalUnplayed ?? 0
    }

    func rustEpisodeCount() -> Int {
        rustLibrarySummary()?.episodeCount ?? 0
    }

    func rustFollowedPodcastCount() -> Int {
        rustLibrarySummary()?.followedPodcastCount ?? 0
    }

    func rustHasUnfollowedPodcasts() -> Bool {
        rustLibrarySummary()?.hasUnfollowedPodcasts ?? false
    }

    private func rustLibrarySummary() -> LibrarySummaryResponse? {
        guard let envelope = kernel?.librarySummaryEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                LibrarySummaryResponse.self,
                from: data
              )
        else { return nil }
        return decoded
    }

    func rustFollowedPodcasts() -> [Podcast] {
        guard let envelope = kernel?.libraryFollowedPodcastsEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                FollowedPodcastsResponse.self,
                from: data
              )
        else { return [] }
        return decoded.podcastIds.compactMap { podcast(id: $0) }
    }

    func rustAllPodcasts(query: String = "") -> [Podcast] {
        guard let envelope = kernel?.libraryAllPodcastsEnvelope(query: query),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                AllPodcastsResponse.self,
                from: data
              )
        else { return [] }
        return decoded.podcastIds.compactMap { podcast(id: $0) }
    }

    func rustOwnedPodcasts() -> [Podcast] {
        guard let envelope = kernel?.libraryOwnedPodcastsEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                OwnedPodcastsResponse.self,
                from: data
              )
        else { return [] }
        return decoded.podcastIds.compactMap { podcast(id: $0) }
    }

    func rustStarredEpisodeIDs() -> [UUID] {
        guard let envelope = kernel?.libraryStarredEpisodesEnvelope(),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                StarredEpisodesResponse.self,
                from: data
              )
        else { return [] }
        return decoded.episodeIds
    }

    func rustEpisodeID(reference: String) -> UUID? {
        guard let envelope = kernel?.libraryEpisodeLookupEnvelope(reference: reference),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                EpisodeLookupResponse.self,
                from: data
              )
        else { return nil }
        return decoded.episodeId
    }

    func rustIsAlreadySubscribed(feedURL: String?, ownerPubkey: String?, podcastID: UUID? = nil) -> Bool {
        guard let envelope = kernel?.librarySubscriptionStatusEnvelope(
            feedURL: feedURL,
            ownerPubkey: ownerPubkey,
            podcastID: podcastID?.uuidString
        ),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                SubscriptionStatusResponse.self,
                from: data
              )
        else { return false }
        return decoded.isAlreadySubscribed
    }

    func rustPodcastForOwnerPubkey(_ ownerPubkey: String) -> Podcast? {
        guard let envelope = kernel?.libraryPodcastForOwnerPubkeyEnvelope(ownerPubkey: ownerPubkey),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                PodcastForOwnerPubkeyResponse.self,
                from: data
              ),
              let podcastID = decoded.podcastId
        else { return nil }
        return podcast(id: podcastID)
    }

    func rustStorageBreakdown(
        files: [EpisodeDownloadStore.OnDiskFile]
    ) -> StorageSettingsView.Snapshot {
        let rows: [[String: Any]] = files.map { file in
            var row: [String: Any] = [
                "url": file.url.path,
                "bytes": file.bytes,
            ]
            if let episodeID = file.episodeID {
                row["episode_id"] = episodeID.uuidString
            }
            return row
        }
        guard let envelope = kernel?.storageBreakdownEnvelope(files: rows),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                StorageBreakdownResponse.self,
                from: data
              )
        else { return .empty }
        return StorageSettingsView.Snapshot(
            totalBytes: decoded.totalBytes,
            shows: decoded.shows.map { row in
                StorageSettingsView.ShowRow(
                    subscriptionID: row.subscriptionID,
                    title: row.title,
                    bytes: row.bytes,
                    episodeCount: row.episodeCount,
                    episodeIDs: row.episodeIds
                )
            },
            orphanBytes: decoded.orphanBytes,
            orphanCount: decoded.orphanCount,
            orphanURLs: decoded.orphanUrls.map { URL(fileURLWithPath: $0) }
        )
    }

    private func rustListenNow(limit: Int) -> ListenNowResponse {
        guard let envelope = kernel?.carplayListenNowEnvelope(limit: limit),
              let data = envelope.data(using: .utf8),
              let decoded = try? JSONDecoder.rustLibraryProjection.decode(
                ListenNowIDsResponse.self,
                from: data
              )
        else {
            return ListenNowResponse(inProgressEpisodes: [], latestEpisodes: [])
        }
        return ListenNowResponse(
            inProgressEpisodes: decoded.inProgressEpisodeIds.compactMap { episode(id: $0) },
            latestEpisodes: decoded.latestEpisodeIds.compactMap { episode(id: $0) }
        )
    }
}

private struct FollowedPodcastsResponse: Decodable {
    let podcastIds: [UUID]
}

private struct AllPodcastsResponse: Decodable {
    let podcastIds: [UUID]
}

private struct OwnedPodcastsResponse: Decodable {
    let podcastIds: [UUID]
}

private struct StarredEpisodesResponse: Decodable {
    let episodeIds: [UUID]
}

private struct EpisodeLookupResponse: Decodable {
    let episodeId: UUID?
}

private struct SubscriptionStatusResponse: Decodable {
    let isAlreadySubscribed: Bool
    let podcastId: UUID?
    let title: String?
    let author: String?
    let feedUrl: String?
    let episodeCount: Int?
}

private struct PodcastForOwnerPubkeyResponse: Decodable {
    let podcastId: UUID?
}

private struct StorageBreakdownResponse: Decodable {
    let totalBytes: Int64
    let shows: [StorageShowRow]
    let orphanBytes: Int64
    let orphanCount: Int
    let orphanUrls: [String]
}

private struct StorageShowRow: Decodable {
    let subscriptionID: UUID
    let title: String
    let bytes: Int64
    let episodeCount: Int
    let episodeIds: [UUID]
}

private struct ShowEpisodesResponse: Decodable {
    let episodeIds: [UUID]
}

private struct ListenNowIDsResponse: Decodable {
    let inProgressEpisodeIds: [UUID]
    let latestEpisodeIds: [UUID]
}

private struct ListenNowResponse {
    let inProgressEpisodes: [Episode]
    let latestEpisodes: [Episode]
}

private struct LibrarySummaryResponse: Decodable {
    let episodeCount: Int
    let followedPodcastCount: Int
    let hasUnfollowedPodcasts: Bool
    let totalUnplayed: Int
}

private struct EpisodeForAudioURLResponse: Decodable {
    let episodeId: UUID?
}

private extension JSONDecoder {
    static let rustLibraryProjection: JSONDecoder = {
        let decoder = JSONDecoder()
        decoder.keyDecodingStrategy = .convertFromSnakeCase
        return decoder
    }()
}
