import Foundation
import os.log

// MARK: - Playback adapter

/// Routes agent playback intent through the Rust player actor. Keeps weak refs
/// so the agent surface never extends the UI lifetime. Native audio execution
/// remains in the capability bridge; agent-visible playback facts come from
/// Rust's `PlayerState` projection.
final class LivePlaybackHostAdapter: PlaybackHostProtocol, @unchecked Sendable {

    private let logger = Logger.app("AgentTools")
    weak var store: AppStateStore?

    init(store: AppStateStore) {
        self.store = store
    }

    func playEpisode(
        episodeID: EpisodeID,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeOutcome {
        await MainActor.run {
            guard let store else {
                logger.error("playEpisode: store unavailable")
                return .unavailable
            }
            let dispatch: DispatchResult?
            switch queuePosition {
            case .now:
                dispatch = store.kernelPlay(
                    episodeID: episodeID,
                    startSeconds: startSeconds,
                    endSeconds: endSeconds
                )
            case .next:
                if let endSeconds {
                    dispatch = store.kernelEnqueueSegmentNext(
                        episodeID: episodeID,
                        startSeconds: startSeconds,
                        endSeconds: endSeconds
                    )
                } else {
                    dispatch = store.kernelEnqueueNext(episodeID: episodeID)
                }
            case .end:
                if let endSeconds {
                    dispatch = store.kernelEnqueueSegmentLast(
                        episodeID: episodeID,
                        startSeconds: startSeconds,
                        endSeconds: endSeconds
                    )
                } else {
                    dispatch = store.kernelEnqueueLast(episodeID: episodeID)
                }
            }
            guard let dispatch else { return .unavailable }
            if case let .failure(message) = dispatch {
                return .rejected(message)
            }
            guard let toolResult = store.kernel?.playbackToolResult(
                episodeID: episodeID,
                queuePosition: queuePosition,
                startedPlaying: queuePosition == .now
            ) else {
                return .unavailable
            }
            guard toolResult.ok else {
                return .rejected(toolResult.message ?? "Playback result was rejected by the kernel.")
            }
            logger.info("playEpisode(\(queuePosition.rawValue, privacy: .public)): \(toolResult.episodeTitle ?? episodeID, privacy: .public)")
            return .played(
                PlayEpisodeResult(
                    episodeID: toolResult.episodeId,
                    queuePosition: QueuePosition(rawValue: toolResult.queuePosition) ?? queuePosition,
                    startedPlaying: toolResult.startedPlaying,
                    episodeTitle: toolResult.episodeTitle,
                    podcastTitle: toolResult.podcastTitle,
                    durationSeconds: toolResult.durationSeconds
                )
            )
        }
    }

    func pausePlayback() async -> Bool {
        await MainActor.run {
            guard let store else {
                logger.error("pausePlayback: store missing")
                return false
            }
            guard case .some(.accepted) = store.kernelPause() else {
                logger.error("pausePlayback: kernel rejected")
                return false
            }
            logger.info("pausePlayback: paused")
            return true
        }
    }

    func setPlaybackRate(_ rate: Double) async -> Double? {
        await MainActor.run {
            guard let store else {
                logger.error("setPlaybackRate: store missing")
                return nil
            }
            guard case .some(.accepted) = store.kernelSetSpeed(rate) else {
                logger.error("setPlaybackRate: kernel rejected")
                return nil
            }
            let applied = store.kernel?.nowPlayingToolResult()?.rate ?? rate
            logger.info("setPlaybackRate: \(applied)")
            return applied
        }
    }

    func setSleepTimer(mode: String, minutes: Int?) async -> String? {
        await MainActor.run {
            guard let store else {
                logger.error("setSleepTimer: store missing")
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
            guard case .some(.accepted) = store.kernelSetSleepTimer(timer) else {
                logger.error("setSleepTimer: kernel rejected")
                return nil
            }
            logger.info("setSleepTimer: \(timer.label, privacy: .public)")
            return timer.label
        }
    }

    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        let parentPlan = await MainActor.run {
            store?.kernel?.externalPlayPlan(feedURLString: feedURLString)
        }
        guard let parentPlan, parentPlan.ok else {
            logger.error("playExternalEpisode: store unavailable")
            return nil
        }
        let result: PlayEpisodeResult? = await MainActor.run {
            guard let store else {
                logger.error("playExternalEpisode: store missing")
                return nil
            }
            if parentPlan.shouldCreatePlaceholder {
                store.kernelCreatePodcast(
                    podcastId: parentPlan.podcastId,
                    title: parentPlan.placeholderTitle ?? parentPlan.feedUrl ?? "Unknown",
                    description: "",
                    author: "",
                    feedUrl: parentPlan.feedUrl,
                    artworkUrl: nil,
                    language: nil,
                    categories: [],
                    visibility: parentPlan.visibility ?? Podcast.NostrVisibility.public.rawValue,
                    titleIsPlaceholder: parentPlan.titleIsPlaceholder
                )
            }
            // Add the episode to the Rust kernel store (SSOT). The enclosure is
            // an `http(s)://` URL, so the kernel marks it NotDownloaded; it
            // rides the next projection push back into `store.episodes`.
            let episodeID = UUID()
            store.kernelAddEpisode(
                podcastId: parentPlan.podcastId,
                episodeId: episodeID.uuidString,
                title: title,
                enclosureUrl: audioURL.absoluteString,
                description: "",
                durationSecs: durationSeconds,
                imageUrl: nil,
                chapters: [],
                transcript: nil
            )
            let dispatch: DispatchResult?
            switch queuePosition {
            case .now:
                dispatch = store.kernelPlay(
                    episodeID: episodeID,
                    startSeconds: startSeconds,
                    endSeconds: endSeconds
                )
            case .next:
                if let endSeconds {
                    dispatch = store.kernelEnqueueSegmentNext(
                        episodeID: episodeID.uuidString,
                        startSeconds: startSeconds,
                        endSeconds: endSeconds
                    )
                } else {
                    dispatch = store.kernelEnqueueNext(episodeID: episodeID)
                }
            case .end:
                if let endSeconds {
                    dispatch = store.kernelEnqueueSegmentLast(
                        episodeID: episodeID.uuidString,
                        startSeconds: startSeconds,
                        endSeconds: endSeconds
                    )
                } else {
                    dispatch = store.kernelEnqueueLast(episodeID: episodeID)
                }
            }
            guard case .some(.accepted) = dispatch else {
                logger.error("playExternalEpisode: kernel rejected")
                return nil
            }
            guard let toolResult = store.kernel?.playbackToolResult(
                episodeID: episodeID.uuidString,
                queuePosition: queuePosition,
                startedPlaying: queuePosition == .now
            ), toolResult.ok else {
                logger.error("playExternalEpisode: Rust playback result unavailable")
                return nil
            }
            logger.info("playExternalEpisode(\(queuePosition.rawValue, privacy: .public)): '\(toolResult.episodeTitle ?? title, privacy: .public)'")
            return PlayEpisodeResult(
                episodeID: toolResult.episodeId,
                queuePosition: QueuePosition(rawValue: toolResult.queuePosition) ?? queuePosition,
                startedPlaying: toolResult.startedPlaying,
                episodeTitle: toolResult.episodeTitle,
                podcastTitle: toolResult.podcastTitle,
                durationSeconds: toolResult.durationSeconds
            )
        }
        // Asynchronously hydrate podcast metadata in the background so the
        // first render shows whatever we have, and later renders pick up
        // real title / artwork once the feed comes back. Fire-and-forget;
        // playback doesn't depend on the result.
        if parentPlan.shouldHydrateMetadata,
           let feedURLString = parentPlan.feedUrl,
           let podcastID = UUID(uuidString: parentPlan.podcastId),
           let url = URL(string: feedURLString) {
            Task.detached { [weak self] in
                await self?.hydratePlaceholderPodcastMetadata(podcastID: podcastID, feedURL: url)
            }
        }
        return result
    }

    func getNowPlaying() async -> NowPlayingState {
        await MainActor.run {
            guard let store,
                  let result = store.kernel?.nowPlayingToolResult(),
                  result.ok else {
                return NowPlayingState(positionSeconds: 0, isPlaying: false, rate: 1.0)
            }
            return NowPlayingState(
                episodeID: result.episodeId,
                episodeTitle: result.episodeTitle,
                podcastID: result.podcastId,
                podcastTitle: result.podcastTitle,
                positionSeconds: result.positionSeconds,
                durationSeconds: result.durationSeconds,
                isPlaying: result.isPlaying,
                rate: result.rate
            )
        }
    }

    func seekTo(positionSeconds: Double) async -> Double? {
        await MainActor.run {
            guard let store else {
                logger.error("seekTo: store missing")
                return nil
            }
            guard case .some(.accepted) = store.kernelSeek(positionSecs: positionSeconds) else {
                logger.error("seekTo: kernel rejected")
                return nil
            }
            let applied = store.kernel?.nowPlayingToolResult()?.positionSeconds ?? positionSeconds
            logger.info("seekTo: \(applied)")
            return applied
        }
    }

    func skipForward(seconds: Double?) async -> Double? {
        await MainActor.run {
            guard let store else {
                logger.error("skipForward: store missing")
                return nil
            }
            guard case .some(.accepted) = store.kernelSkipForward(secs: seconds) else {
                logger.error("skipForward: kernel rejected")
                return nil
            }
            logger.info("skipForward: \(seconds?.description ?? "default")")
            return store.kernel?.nowPlaying?.positionSecs ?? 0
        }
    }

    func skipBackward(seconds: Double?) async -> Double? {
        await MainActor.run {
            guard let store else {
                logger.error("skipBackward: store missing")
                return nil
            }
            guard case .some(.accepted) = store.kernelSkipBackward(secs: seconds) else {
                logger.error("skipBackward: kernel rejected")
                return nil
            }
            logger.info("skipBackward: \(seconds?.description ?? "default")")
            return store.kernel?.nowPlaying?.positionSecs ?? 0
        }
    }

    /// Fetches the feed in the background and updates the placeholder
    /// `Podcast` row's title / author / artwork. Does NOT upsert episodes:
    /// the user hasn't followed this show, so we keep the library
    /// untouched (the user's external-played episode already exists).
    private func hydratePlaceholderPodcastMetadata(podcastID: UUID, feedURL: URL) async {
        guard let store else { return }
        let client = FeedClient()
        // Seed the fetch with a placeholder that carries the known podcast ID
        // so FeedClient preserves foreign keys. titleIsPlaceholder stays true
        // here; we clear it only when the fetch returns real metadata below.
        let placeholder = Podcast(
            id: podcastID,
            feedURL: feedURL,
            title: feedURL.host ?? feedURL.absoluteString,
            titleIsPlaceholder: true
        )
        do {
            let result = try await client.fetch(placeholder)
            if case .updated(let podcast, _, _) = result {
                // Re-create with the SAME id + enriched metadata; the kernel
                // upsert updates the row in place. `titleIsPlaceholder: false`
                // clears the provisional marker now that real metadata arrived.
                await MainActor.run {
                    store.kernelCreatePodcast(
                        podcastId: podcastID.uuidString,
                        title: podcast.title,
                        description: podcast.description,
                        author: podcast.author,
                        feedUrl: feedURL.absoluteString,
                        artworkUrl: podcast.imageURL?.absoluteString,
                        language: podcast.language,
                        categories: podcast.categories,
                        visibility: Podcast.NostrVisibility.public.rawValue,
                        titleIsPlaceholder: false
                    )
                }
            }
            // .notModified on a first hydration fetch means the server sent no
            // real title; leave titleIsPlaceholder = true in the store so the
            // next refresh can retry.
        } catch {
            logger.error(
                "playExternalEpisode: background metadata fetch failed for \(feedURL.absoluteString, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            // titleIsPlaceholder remains true in the store; the next refresh
            // attempt will retry hydration.
        }
    }
}
