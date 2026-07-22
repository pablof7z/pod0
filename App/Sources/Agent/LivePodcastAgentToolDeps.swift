import Foundation
import os.log

// MARK: - LivePodcastAgentToolDeps
//
// Wires the lane-10 podcast tool surface (`AgentTools.dispatchPodcast`) to the
// real services that ship in the app:
//
//   • `PodcastAgentKnowledgeSearchProtocol` → `LivePodcastKnowledgeAdapter`
//   • `EpisodeSummarizerProtocol`      → `LiveEpisodeSummarizerAdapter`
//   • `EpisodeFetcherProtocol`         → `LiveEpisodeFetcherAdapter`
//   • `PlaybackHostProtocol`           → `LivePlaybackHostAdapter`
//   • `PerplexityClientProtocol`       → `PerplexityClient`
//   • `TTSPublisherProtocol`           → `AgentTTSComposer`
//
// Constructed once per `AgentChatSession`, the bundle holds weak references
// to `AppStateStore` and `PlaybackState` so the agent adapters never extend
// their lifetimes. Heavy adapters (knowledge recall, summarizer) live in their own files;
// the small ones live here.

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
            knowledge: LivePodcastKnowledgeAdapter(store: store),
            summarizer: LiveEpisodeSummarizerAdapter(
                store: store,
                transcriptReader: store.transcriptReader
            ),
            fetcher: LiveEpisodeFetcherAdapter(store: store),
            playback: LivePlaybackHostAdapter(store: store, playback: playback),
            library: LivePodcastLibraryAdapter(
                store: store,
                transcriptService: .shared,
                refreshService: .shared,
                transcriptReader: store.transcriptReader
            ),
            inventory: inventory,
            categories: inventory,
            perplexity: PerplexityClient(),
            ttsPublisher: AgentTTSComposer(store: store, playback: playback),
            directory: LivePodcastDirectoryAdapter(),
            subscribe: LivePodcastSubscribeAdapter(store: store),
            youtubeIngestion: LiveYouTubeIngestionAdapter(store: store),
            ownedPodcasts: LiveAgentOwnedPodcastManager(store: store)
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

// MARK: - Playback adapter

/// Drives the live `PlaybackState` from agent tool calls. Uses weak refs so
/// the agent surface never extends the player's lifetime past the SwiftUI
/// scene that owns it.
final class LivePlaybackHostAdapter: PlaybackHostProtocol, @unchecked Sendable {

    let logger = Logger.app("AgentTools")
    weak var store: AppStateStore?
    weak var playback: PlaybackState?

    init(store: AppStateStore, playback: PlaybackState) {
        self.store = store
        self.playback = playback
    }

    func playEpisode(
        episodeID: EpisodeID,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        await MainActor.run {
            guard let store, let playback,
                  let uuid = UUID(uuidString: episodeID),
                  let episode = store.episode(id: uuid) else {
                logger.error("playEpisode: unknown episode \(episodeID, privacy: .public)")
                return nil
            }
            let item = QueueItem(
                episodeID: uuid,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                label: nil
            )
            let podcastTitle = store.podcast(id: episode.podcastID)?.title
            switch queuePosition {
            case .now:
                // Replace current playback with this item; existing queue is
                // preserved and resumes after this finishes.
                playback.enqueueSegments([item], playNow: true)
                logger.info("playEpisode(now): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .now,
                    startedPlaying: true,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .next:
                playback.insertNext(item)
                logger.info("playEpisode(next): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .next,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .end:
                playback.enqueueItem(item)
                logger.info("playEpisode(end): \(episode.title, privacy: .public)")
                return PlayEpisodeResult(
                    episodeID: episodeID,
                    queuePosition: .end,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            }
        }
    }

    func pausePlayback() async -> Bool {
        await MainActor.run {
            guard let playback else {
                logger.error("pausePlayback: playback host missing")
                return false
            }
            playback.pause()
            logger.info("pausePlayback: paused")
            return true
        }
    }

    func setPlaybackRate(_ rate: Double) async -> Double? {
        await MainActor.run {
            guard let playback else {
                logger.error("setPlaybackRate: playback host missing")
                return nil
            }
            let clamped = min(max(rate, 0.5), 3.0)
            playback.setRate(clamped)
            logger.info("setPlaybackRate: \(clamped)")
            return clamped
        }
    }

    func setSleepTimer(mode: String, minutes: Int?) async -> String? {
        await MainActor.run {
            guard let playback else {
                logger.error("setSleepTimer: playback host missing")
                return nil
            }
            let timer: PlaybackSleepTimer
            switch mode {
            case "off":
                timer = .off
            case "end_of_episode":
                timer = .endOfEpisode
            case "minutes":
                timer = .minutes(max(1, minutes ?? 30))
            default:
                timer = .off
            }
            playback.setSleepTimer(timer)
            logger.info("setSleepTimer: \(timer.label, privacy: .public)")
            return timer.label
        }
    }

}

// MARK: - Library adapter

enum PodcastAgentToolAdapterError: LocalizedError {
    case unavailable(String)
    case invalidID(String)
    case missingEpisode(String)
    case missingPodcast(String)
    case invalidClipBounds

    var errorDescription: String? {
        switch self {
        case .unavailable(let name): return "\(name) is unavailable."
        case .invalidID(let value): return "Invalid UUID: \(value)"
        case .missingEpisode(let id): return "Episode not found: \(id)"
        case .missingPodcast(let id): return "Podcast not found: \(id)"
        case .invalidClipBounds: return "Clip timestamps are outside the supported range."
        }
    }
}
