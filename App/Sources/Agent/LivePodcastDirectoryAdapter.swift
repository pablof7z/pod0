import Foundation

// MARK: - LivePodcastDirectoryAdapter

// Implements `PodcastDirectoryProtocol` through the Rust-owned Apple Podcasts
// directory FFI. Swift supplies user intent and decodes the result envelope;
// Rust owns endpoint shape, limit clamping, HTTP capability dispatch, and JSON
// response parsing.
struct LivePodcastDirectoryAdapter: PodcastDirectoryProtocol, @unchecked Sendable {

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func searchDirectory(
        query: String,
        type: PodcastDirectorySearchType,
        limit: Int
    ) async throws -> [PodcastDirectoryHit] {
        let envelope = await MainActor.run {
            store?.kernel?.itunesDirectorySearchEnvelope(
                query: query,
                type: type.rawValue,
                limit: limit
            )
        }
        guard let envelope else {
            throw DirectoryError.unavailable("KernelModel")
        }
        return try Self.decodeSearchEnvelope(envelope)
    }

    func lookupFeedURL(forCollectionID collectionID: String) async throws -> String? {
        let envelope = await MainActor.run {
            store?.kernel?.itunesLookupFeedEnvelope(collectionID: collectionID)
        }
        guard let envelope else {
            throw DirectoryError.unavailable("KernelModel")
        }
        return try Self.decodeLookupEnvelope(envelope)
    }

    // MARK: - Envelope decoding

    private static func decodeSearchEnvelope(_ envelope: String) throws -> [PodcastDirectoryHit] {
        guard let data = envelope.data(using: .utf8) else {
            throw DirectoryError.parseError("Directory search returned non-UTF8 data")
        }
        let decoded = try JSONDecoder().decode(DirectorySearchEnvelope.self, from: data)
        if let error = decoded.error {
            throw DirectoryError.parseError(error)
        }
        return (decoded.result ?? []).map(\.hit)
    }

    private static func decodeLookupEnvelope(_ envelope: String) throws -> String? {
        guard let data = envelope.data(using: .utf8) else {
            throw DirectoryError.parseError("Directory lookup returned non-UTF8 data")
        }
        let decoded = try JSONDecoder().decode(DirectoryLookupEnvelope.self, from: data)
        if let error = decoded.error {
            throw DirectoryError.parseError(error)
        }
        return decoded.feedURL
    }

    private struct DirectorySearchEnvelope: Decodable {
        var result: [DirectoryHitDTO]?
        var error: String?
    }

    private struct DirectoryLookupEnvelope: Decodable {
        var feedURL: String?
        var error: String?

        private enum CodingKeys: String, CodingKey {
            case feedURL = "feed_url"
            case error
        }
    }

    private struct DirectoryHitDTO: Decodable {
        var collectionID: Int?
        var podcastTitle: String
        var author: String?
        var feedURL: String?
        var artworkURL: String?
        var episodeTitle: String?
        var episodeAudioURL: String?
        var episodeGUID: String?
        var episodePublishedAt: Int?
        var episodeDurationSeconds: Int?
        var episodeDescription: String?

        private enum CodingKeys: String, CodingKey {
            case collectionID = "collection_id"
            case podcastTitle = "podcast_title"
            case author
            case feedURL = "feed_url"
            case artworkURL = "artwork_url"
            case episodeTitle = "episode_title"
            case episodeAudioURL = "episode_audio_url"
            case episodeGUID = "episode_guid"
            case episodePublishedAt = "episode_published_at"
            case episodeDurationSeconds = "episode_duration_seconds"
            case episodeDescription = "episode_description"
        }

        var hit: PodcastDirectoryHit {
            PodcastDirectoryHit(
                collectionID: collectionID,
                podcastTitle: podcastTitle,
                author: author,
                feedURL: feedURL,
                artworkURL: artworkURL,
                episodeTitle: episodeTitle,
                episodeAudioURL: episodeAudioURL,
                episodeGUID: episodeGUID,
                episodePublishedAt: episodePublishedAt.map {
                    Date(timeIntervalSince1970: TimeInterval($0))
                },
                episodeDurationSeconds: episodeDurationSeconds,
                episodeDescription: episodeDescription
            )
        }
    }
}

// MARK: - LivePodcastSubscribeAdapter

/// Implements `PodcastSubscribeProtocol` using `SubscriptionService`.
final class LivePodcastSubscribeAdapter: PodcastSubscribeProtocol, @unchecked Sendable {

    private struct RustSubscriptionStatus: Decodable {
        let isAlreadySubscribed: Bool
        let podcastID: String?
        let title: String?
        let author: String?
        let feedURL: String?
        let episodeCount: Int?

        enum CodingKeys: String, CodingKey {
            case isAlreadySubscribed = "is_already_subscribed"
            case podcastID = "podcast_id"
            case title
            case author
            case feedURL = "feed_url"
            case episodeCount = "episode_count"
        }
    }

    private struct RustPodcastSubscribeSnapshot: Decodable {
        let shouldSubscribe: Bool
        private let resultDTO: RustPodcastSubscribeResult?

        var result: PodcastSubscribeResult? {
            resultDTO?.result
        }

        enum CodingKeys: String, CodingKey {
            case shouldSubscribe = "should_subscribe"
            case resultDTO = "result"
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            shouldSubscribe = try c.decodeIfPresent(Bool.self, forKey: .shouldSubscribe) ?? false
            resultDTO = try c.decodeIfPresent(RustPodcastSubscribeResult.self, forKey: .resultDTO)
        }
    }

    private struct RustPodcastSubscribeResult: Decodable {
        let podcastID: String
        let title: String
        let author: String?
        let feedURL: String
        let episodeCount: Int
        let alreadySubscribed: Bool

        enum CodingKeys: String, CodingKey {
            case podcastID = "podcast_id"
            case title
            case author
            case feedURL = "feed_url"
            case episodeCount = "episode_count"
            case alreadySubscribed = "already_subscribed"
        }

        var result: PodcastSubscribeResult {
            PodcastSubscribeResult(
                podcastID: podcastID,
                title: title,
                author: author,
                feedURL: feedURL,
                episodeCount: episodeCount,
                alreadySubscribed: alreadySubscribed
            )
        }
    }

    private struct RustPodcastDeleteSnapshot: Decodable {
        let error: String?
        private let resultDTO: RustPodcastDeleteResult?

        var result: PodcastDeleteResult? { resultDTO?.result }

        enum CodingKeys: String, CodingKey {
            case error
            case resultDTO = "result"
        }

        init(from decoder: Decoder) throws {
            let c = try decoder.container(keyedBy: CodingKeys.self)
            error = try c.decodeIfPresent(String.self, forKey: .error)
            resultDTO = try c.decodeIfPresent(RustPodcastDeleteResult.self, forKey: .resultDTO)
        }
    }

    /// Wire-shape mirror of `PodcastDeleteResult`. The domain type is
    /// `Sendable, Equatable` (not `Decodable`), so the JSON envelope is decoded
    /// into this DTO and mapped, matching the `RustPodcastSubscribeResult`
    /// pattern above.
    private struct RustPodcastDeleteResult: Decodable {
        let podcastID: String
        let title: String?
        let wasSubscribed: Bool
        let episodesDeleted: Int

        enum CodingKeys: String, CodingKey {
            case podcastID = "podcast_id"
            case title
            case wasSubscribed = "was_subscribed"
            case episodesDeleted = "episodes_deleted"
        }

        var result: PodcastDeleteResult {
            PodcastDeleteResult(
                podcastID: podcastID,
                title: title,
                wasSubscribed: wasSubscribed,
                episodesDeleted: episodesDeleted
            )
        }
    }

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func subscribe(feedURLString: String) async throws -> PodcastSubscribeResult {
        guard let store else {
            throw DirectoryError.unavailable("AppStateStore")
        }
        let normalizedFeedURL = SubscriptionService.normalizedFeedURL(from: feedURLString)
        if let feedURL = normalizedFeedURL {
            let status = await subscriptionStatus(feedURL: feedURL.absoluteString, store: store)
            if let result = try Self.subscribeSnapshot(
                normalizedFeedURL: feedURL.absoluteString,
                podcastID: status?.podcastID,
                title: status?.title,
                author: status?.author,
                feedURL: status?.feedURL,
                episodeCount: status?.episodeCount,
                isAlreadySubscribed: status?.isAlreadySubscribed ?? false,
                completed: false
            ).result {
                return result
            }
        }
        let service = await MainActor.run { SubscriptionService(store: store) }
        let podcast = try await service.addSubscription(feedURLString: feedURLString)
        let count = await MainActor.run { store.rustEpisodeCount(forPodcast: podcast.id) }
        let snapshot = try Self.subscribeSnapshot(
            normalizedFeedURL: normalizedFeedURL?.absoluteString ?? feedURLString,
            podcastID: podcast.id.uuidString,
            title: podcast.title,
            author: podcast.author,
            feedURL: podcast.feedURL?.absoluteString,
            episodeCount: count,
            isAlreadySubscribed: false,
            completed: true
        )
        guard let result = snapshot.result else {
            throw DirectoryError.parseError("subscribe_podcast policy response was incomplete.")
        }
        return result
    }

    private static func subscribeSnapshot(
        normalizedFeedURL: String,
        podcastID: String?,
        title: String?,
        author: String?,
        feedURL: String?,
        episodeCount: Int?,
        isAlreadySubscribed: Bool,
        completed: Bool
    ) throws -> RustPodcastSubscribeSnapshot {
        var request: [String: Any] = [
            "op": "subscribe_snapshot",
            "normalized_feed_url": normalizedFeedURL,
            "is_already_subscribed": isAlreadySubscribed,
            "completed": completed,
        ]
        if let podcastID { request["podcast_id"] = podcastID }
        if let title { request["title"] = title }
        if let author, !author.isEmpty { request["author"] = author }
        if let feedURL { request["feed_url"] = feedURL }
        if let episodeCount { request["episode_count"] = episodeCount }
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else {
            throw DirectoryError.parseError("Could not encode subscribe_podcast request.")
        }
        return try json.withCString { ptr -> RustPodcastSubscribeSnapshot in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                throw DirectoryError.unavailable("subscribe_podcast policy")
            }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(RustPodcastSubscribeSnapshot.self, from: data)
            else {
                throw DirectoryError.parseError("Could not decode subscribe_podcast policy response.")
            }
            return decoded
        }
    }

    private func subscriptionStatus(feedURL: String, store: AppStateStore) async -> RustSubscriptionStatus? {
        await MainActor.run {
            guard let envelope = store.kernel?.librarySubscriptionStatusEnvelope(
                feedURL: feedURL,
                ownerPubkey: nil,
                podcastID: nil
            ),
                  let data = envelope.data(using: .utf8)
            else { return nil }
            return try? JSONDecoder().decode(RustSubscriptionStatus.self, from: data)
        }
    }

    private func subscriptionStatus(podcastID: UUID, store: AppStateStore) async -> RustSubscriptionStatus? {
        await MainActor.run {
            guard let envelope = store.kernel?.librarySubscriptionStatusEnvelope(
                feedURL: nil,
                ownerPubkey: nil,
                podcastID: podcastID.uuidString
            ),
                  let data = envelope.data(using: .utf8)
            else { return nil }
            return try? JSONDecoder().decode(RustSubscriptionStatus.self, from: data)
        }
    }

    func ensurePodcast(feedURLString: String) async throws -> PodcastEnsureResult {
        guard let store else {
            throw DirectoryError.unavailable("AppStateStore")
        }
        let service = await MainActor.run { SubscriptionService(store: store) }
        let podcast = try await service.ensurePodcast(feedURLString: feedURLString)
        let count = await MainActor.run { store.rustEpisodeCount(forPodcast: podcast.id) }
        let resolvedFeedURL = podcast.feedURL?.absoluteString ?? feedURLString
        return PodcastEnsureResult(
            podcastID: podcast.id.uuidString,
            title: podcast.title,
            author: podcast.author.isEmpty ? nil : podcast.author,
            feedURL: resolvedFeedURL,
            episodeCount: count
        )
    }

    func unfollowPodcast(podcastID: PodcastID) async throws -> PodcastUnfollowResult {
        guard let store else {
            throw DirectoryError.unavailable("AppStateStore")
        }
        guard let uuid = UUID(uuidString: podcastID) else {
            throw DirectoryError.parseError("Invalid podcast_id: \(podcastID)")
        }
        let status = await subscriptionStatus(podcastID: uuid, store: store)
        return await MainActor.run {
            let title = store.podcast(id: uuid)?.title
            store.kernelUnfollow(podcastID: uuid)
            return PodcastUnfollowResult(
                podcastID: podcastID,
                title: title,
                wasSubscribed: status?.isAlreadySubscribed ?? false
            )
        }
    }

    func deletePodcast(podcastID: PodcastID) async throws -> PodcastDeleteResult {
        guard let store else {
            throw DirectoryError.unavailable("AppStateStore")
        }
        guard let uuid = UUID(uuidString: podcastID) else {
            throw DirectoryError.parseError("Invalid podcast_id: \(podcastID)")
        }
        let status = await subscriptionStatus(podcastID: uuid, store: store)
        return try await MainActor.run {
            let podcast = store.podcast(id: uuid)
            let result = try Self.deleteSnapshot(
                podcastID: podcastID,
                title: podcast?.title,
                wasSubscribed: status?.isAlreadySubscribed ?? false,
                episodesDeleted: store.rustEpisodeCount(forPodcast: uuid)
            )
            store.deletePodcast(podcastID: uuid)
            return result
        }
    }

    private static func deleteSnapshot(
        podcastID: PodcastID,
        title: String?,
        wasSubscribed: Bool,
        episodesDeleted: Int
    ) throws -> PodcastDeleteResult {
        var request: [String: Any] = [
            "op": "delete_podcast_snapshot",
            "podcast_id": podcastID,
            "was_subscribed": wasSubscribed,
            "episodes_deleted": episodesDeleted,
        ]
        if let title { request["title"] = title }
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else {
            throw DirectoryError.parseError("Could not encode delete_podcast request.")
        }
        return try json.withCString { ptr -> PodcastDeleteResult in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                throw DirectoryError.unavailable("delete_podcast policy")
            }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(RustPodcastDeleteSnapshot.self, from: data)
            else {
                throw DirectoryError.parseError("Could not decode delete_podcast policy response.")
            }
            if let error = decoded.error {
                throw DirectoryError.parseError(error)
            }
            guard let snapshot = decoded.result else {
                throw DirectoryError.parseError("delete_podcast policy response was incomplete.")
            }
            return snapshot
        }
    }
}

// MARK: - Error types

enum DirectoryError: LocalizedError {
    case badURL
    case http(Int)
    case parseError(String)
    case unavailable(String)

    var errorDescription: String? {
        switch self {
        case .badURL:            return "Could not construct directory search URL."
        case .http(let code):   return "iTunes Search API returned HTTP \(code)."
        case .parseError(let m): return "Directory parse error: \(m)"
        case .unavailable(let n): return "\(n) is unavailable."
        }
    }
}
