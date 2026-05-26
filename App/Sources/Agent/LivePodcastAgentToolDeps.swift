import Foundation
import os.log

// MARK: - LivePodcastAgentToolDeps
//
// Wires the lane-10 podcast tool surface (`AgentTools.dispatchPodcast`) to the
// real services that ship in the app:
//
//   • `PodcastAgentRAGSearchProtocol`  → `LivePodcastRAGAdapter`
//   • `WikiStorageProtocol`            → `LiveWikiStorageAdapter`
//   • `BriefingComposerProtocol`       → `LiveBriefingComposerAdapter`
//   • `EpisodeSummarizerProtocol`      → `LiveEpisodeSummarizerAdapter`
//   • `EpisodeFetcherProtocol`         → `LiveEpisodeFetcherAdapter`
//   • `PlaybackHostProtocol`           → `LivePlaybackHostAdapter`
//   • `PeerEventPublisherProtocol`     → `LivePeerEventPublisher`
//   • `PerplexityClientProtocol`       → `PerplexityClient`
//   • `TTSPublisherProtocol`           → `AgentTTSComposer`
//
// Constructed once per `AgentChatSession` / `AgentRelayBridge`, the bundle
// holds weak references to `AppStateStore` and `PlaybackState` so the agent
// adapters never extend their lifetimes. Heavy adapters (RAG, Briefing,
// Summarizer) live in their own files; the small ones live here.

@MainActor
enum LivePodcastAgentToolDeps {

    static let logger = Logger.app("AgentTools")

    /// Build a `PodcastAgentToolDeps` bundle wired to the live services.
    /// Call once at session construction; pass the result through to
    /// `AgentTools.dispatchPodcast(...)` for every tool call.
    static func make(
        store: AppStateStore,
        playback: PlaybackState
    ) -> PodcastAgentToolDeps {
        let inventory = LivePodcastInventoryAdapter(store: store)
        return PodcastAgentToolDeps(
            rag: LivePodcastRAGAdapter(store: store),
            wiki: LiveWikiStorageAdapter(store: store),
            briefing: LiveBriefingComposerAdapter(store: store),
            summarizer: LiveEpisodeSummarizerAdapter(store: store),
            fetcher: LiveEpisodeFetcherAdapter(store: store),
            playback: LivePlaybackHostAdapter(store: store, playback: playback),
            library: LivePodcastLibraryAdapter(
                store: store,
                downloadService: .shared,
                transcriptService: .shared,
                refreshService: .shared
            ),
            inventory: inventory,
            categories: inventory,
            peerPublisher: LivePeerEventPublisher(store: store),
            friendDirectory: LiveFriendDirectoryAdapter(store: store),
            pendingRegistrar: LivePendingFriendMessageRegistrar(store: store),
            perplexity: PerplexityClient(),
            ttsPublisher: AgentTTSComposer(store: store, playback: playback),
            directory: LivePodcastDirectoryAdapter(),
            subscribe: LivePodcastSubscribeAdapter(store: store),
            youtubeIngestion: LiveYouTubeIngestionAdapter(store: store),
            ownedPodcasts: LiveAgentOwnedPodcastManager(store: store),
            peerContext: nil
        )
    }
}

// MARK: - Fetcher adapter

/// Resolves episode existence + display metadata from the in-memory
/// `AppStateStore`. Fast — every lookup is a linear scan over the episode
/// array, but the array is bounded by user subscriptions so this is fine.
struct LiveEpisodeFetcherAdapter: EpisodeFetcherProtocol {

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func episodeExists(episodeID: EpisodeID) async -> Bool {
        guard let uuid = UUID(uuidString: episodeID) else { return false }
        return await store?.episode(id: uuid) != nil
    }

    func episodeMetadata(
        episodeID: EpisodeID
    ) async -> (podcastTitle: String, episodeTitle: String, durationSeconds: Int?)? {
        guard let store, let uuid = UUID(uuidString: episodeID),
              let episode = await store.episode(id: uuid) else { return nil }
        let podcast = await store.podcast(id: episode.podcastID)
        return (
            podcastTitle: podcast?.title ?? "",
            episodeTitle: episode.title,
            durationSeconds: episode.duration.map { Int($0) }
        )
    }

    func episodeIDForAudioURL(_ audioURLString: String, podcastID: PodcastID) async -> EpisodeID? {
        guard let store, let podcastUUID = UUID(uuidString: podcastID) else { return nil }
        let episodes = await store.episodes(forPodcast: podcastUUID)
        return episodes.first { $0.enclosureURL.absoluteString == audioURLString }?.id.uuidString
    }
}

// MARK: - Shared error type

/// Errors thrown by podcast agent tool adapters when they cannot locate a
/// required resource or the store is unavailable. Shared across
/// `LivePlaybackHostAdapter`, `LivePodcastLibraryAdapter`, and
/// `LivePodcastInventoryAdapter`.
enum PodcastAgentToolAdapterError: LocalizedError {
    case unavailable(String)
    case invalidID(String)
    case missingEpisode(String)
    case missingPodcast(String)

    var errorDescription: String? {
        switch self {
        case .unavailable(let name): return "\(name) is unavailable."
        case .invalidID(let value): return "Invalid UUID: \(value)"
        case .missingEpisode(let id): return "Episode not found: \(id)"
        case .missingPodcast(let id): return "Podcast not found: \(id)"
        }
    }
}
