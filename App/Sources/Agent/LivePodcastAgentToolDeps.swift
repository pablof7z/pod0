import Foundation
import os.log

// MARK: - LivePodcastAgentToolDeps
//
// Wires the lane-10 podcast tool surface (`AgentTools.dispatchPodcast`) to the
// real services that ship in the app:
//
//   • `PodcastAgentRAGSearchProtocol`  → `LivePodcastRAGAdapter`
//   • `EpisodeSummaryProviding`        → `LiveEpisodeSummaryAdapter` (kernel)
//   • `EpisodeFetcherProtocol`         → `LiveEpisodeFetcherAdapter`
//   • `PlaybackHostProtocol`           → `LivePlaybackHostAdapter`
//   • `PeerEventPublisherProtocol`     → `LivePeerEventPublisher`
//   • `PerplexityClientProtocol`       → `PerplexityClient`
//   • `TTSPublisherProtocol`           → `AgentTTSComposer`
//
// Constructed once per `AgentChatSession` / `AgentRelayBridge`, the bundle
// holds weak references to `AppStateStore` and `PlaybackState` so the agent
// adapters never extend their lifetimes. Playback intent and agent-visible
// playback state route through Rust.
// Heavy adapters (RAG) live in their own files; the small ones live here.

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
            summarizer: LiveEpisodeSummaryAdapter(store: store),
            fetcher: LiveEpisodeFetcherAdapter(store: store),
            playback: LivePlaybackHostAdapter(store: store),
            library: LivePodcastLibraryAdapter(
                store: store,
                transcriptService: .shared
            ),
            inventory: inventory,
            categories: inventory,
            peerPublisher: LivePeerEventPublisher(store: store),
            friendDirectory: LiveFriendDirectoryAdapter(store: store),
            pendingRegistrar: LivePendingFriendMessageRegistrar(store: store),
            perplexity: PerplexityClient(),
            ttsPublisher: AgentTTSComposer(store: store),
            directory: LivePodcastDirectoryAdapter(store: store),
            subscribe: LivePodcastSubscribeAdapter(store: store),
            youtubeIngestion: LiveYouTubeIngestionAdapter(store: store),
            ownedPodcasts: LiveAgentOwnedPodcastManager(store: store),
            peerContext: nil
        )
    }
}

// MARK: - Summary adapter

/// Forwards `summarize_episode` to the Rust kernel LLM pipeline
/// (`podcast.summarize_episode`) and awaits the result on the snapshot
/// projection (`AppStateStore.kernelSummarizeEpisode`). Replaces the deleted
/// Swift `LiveEpisodeSummarizerAdapter`, which ran its own OpenRouter call.
///
/// On a kernel miss, Swift supplies the publisher description as a raw fact and
/// Rust decides whether that fallback is a valid summary outcome.
struct LiveEpisodeSummaryAdapter: EpisodeSummaryProviding {
    private struct RustSummaryPolicy: Decodable {
        let outcome: String
        let summary: String?
        let message: String?
    }

    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func summarize(episodeID: EpisodeID) async -> EpisodeSummaryOutcome {
        guard let store else { return .unavailable }
        let outcome = await store.kernelSummarizeEpisode(episodeID: episodeID)
        let description: String?
        if let uuid = UUID(uuidString: episodeID) {
            description = await store.episode(id: uuid)?.description
        } else {
            description = nil
        }
        return Self.summaryPolicy(
            summary: outcome.summary,
            error: outcome.error,
            publisherDescription: description
        )
    }

    private static func summaryPolicy(
        summary: String?,
        error: String?,
        publisherDescription: String?
    ) -> EpisodeSummaryOutcome {
        var request: [String: Any] = ["op": "episode_summary_policy"]
        if let summary { request["summary"] = summary }
        if let error { request["error"] = error }
        if let publisherDescription { request["publisher_description"] = publisherDescription }
        guard let data = try? JSONSerialization.data(withJSONObject: request),
              let json = String(data: data, encoding: .utf8)
        else { return .unavailable }
        return json.withCString { ptr -> EpisodeSummaryOutcome in
            guard let result = nmp_app_podcast_agent_action_policy(ptr) else {
                return .unavailable
            }
            defer { nmp_free_string(result) }
            let envelope = String(cString: result)
            guard let data = envelope.data(using: .utf8),
                  let decoded = try? JSONDecoder().decode(RustSummaryPolicy.self, from: data)
            else { return .unavailable }
            switch decoded.outcome {
            case "summary":
                return decoded.summary.map(EpisodeSummaryOutcome.summary) ?? .unavailable
            case "rejected":
                return .rejected(decoded.message ?? "")
            default:
                return .unavailable
            }
        }
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
        return await MainActor.run {
            store.rustEpisodeIDForAudioURL(audioURLString, podcastID: podcastUUID)?.uuidString
        }
    }
}

// MARK: - Shared error type

/// Errors thrown by podcast agent tool adapters when they cannot locate a
/// required resource or the store is unavailable. Shared across
/// `LivePlaybackHostAdapter`, `LivePodcastLibraryAdapter`, and
/// `LivePodcastInventoryAdapter`.
enum PodcastAgentToolAdapterError: LocalizedError {
    case unavailable(String)
    case rejected(String)
    case invalidID(String)
    case missingEpisode(String)
    case missingPodcast(String)

    var errorDescription: String? {
        switch self {
        case .unavailable(let name): return "\(name) is unavailable."
        case .rejected(let message): return message
        case .invalidID(let value): return "Invalid UUID: \(value)"
        case .missingEpisode(let id): return "Episode not found: \(id)"
        case .missingPodcast(let id): return "Podcast not found: \(id)"
        }
    }
}
