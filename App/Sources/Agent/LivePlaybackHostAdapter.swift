import Foundation
import os.log

// MARK: - Playback adapter

/// Drives the live `PlaybackState` from agent tool calls. Uses weak refs so
/// the agent surface never extends the player's lifetime past the SwiftUI
/// scene that owns it.
final class LivePlaybackHostAdapter: PlaybackHostProtocol, @unchecked Sendable {

    private let logger = Logger.app("AgentTools")
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
                playback.enqueueSegments([item], playNow: true) { store.episode(id: $0) }
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
            playback.engine.setRate(clamped)
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

    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        // Resolve which podcast to attach this episode to WITHOUT blocking
        // playback on a network fetch. Three cases:
        //   1. We already know about this feed (existing Podcast row) → use it.
        //   2. We don't know about it yet and a feed_url was supplied → use a
        //      thin placeholder Podcast(feedURL: …) now, then enrich its
        //      metadata in the background. The episode lives under that
        //      placeholder ID across the enrichment hop so its parent is
        //      stable for the user.
        //   3. No feed_url at all → parent to Podcast.unknownID.
        //
        // We deliberately never call `ensurePodcast` here: that helper also
        // upserts every parsed episode in the feed, which would dump the
        // show's whole backlog into the user's library without them having
        // subscribed. Backlog ingestion is reserved for `subscribe_podcast`.
        let parentResolution = await resolveExternalParent(feedURLString: feedURLString)
        guard let parentResolution else {
            logger.error("playExternalEpisode: store unavailable")
            return nil
        }
        let result: PlayEpisodeResult? = await MainActor.run {
            guard let store, let playback else {
                logger.error("playExternalEpisode: playback host missing")
                return nil
            }
            let episode = store.upsertEpisode(
                podcastID: parentResolution.podcastID,
                audioURL: audioURL,
                title: title,
                imageURL: nil,
                duration: durationSeconds
            )
            let podcastTitle = store.podcast(id: parentResolution.podcastID)?.title
            let item = QueueItem(
                episodeID: episode.id,
                startSeconds: startSeconds,
                endSeconds: endSeconds,
                label: nil
            )
            switch queuePosition {
            case .now:
                playback.enqueueSegments([item], playNow: true) { store.episode(id: $0) }
                logger.info("playExternalEpisode(now): '\(title, privacy: .public)' at \(startSeconds ?? 0)")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .now,
                    startedPlaying: true,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .next:
                playback.insertNext(item)
                logger.info("playExternalEpisode(next): '\(title, privacy: .public)'")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .next,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            case .end:
                playback.enqueueItem(item)
                logger.info("playExternalEpisode(end): '\(title, privacy: .public)'")
                return PlayEpisodeResult(
                    episodeID: episode.id.uuidString,
                    queuePosition: .end,
                    startedPlaying: false,
                    episodeTitle: episode.title,
                    podcastTitle: podcastTitle,
                    durationSeconds: episode.duration.map { Int($0) }
                )
            }
        }
        // Asynchronously hydrate podcast metadata in the background so the
        // first render shows whatever we have, and later renders pick up
        // real title / artwork once the feed comes back. Fire-and-forget;
        // playback doesn't depend on the result.
        if let feedURLString,
           parentResolution.shouldHydrateMetadata,
           let url = URL(string: feedURLString) {
            Task.detached { [weak self] in
                await self?.hydratePlaceholderPodcastMetadata(podcastID: parentResolution.podcastID, feedURL: url)
            }
        }
        return result
    }

    /// Decision wrapper: which podcast ID to parent the episode to RIGHT
    /// NOW, plus whether the caller should kick off a background metadata
    /// fetch to enrich a freshly-created placeholder.
    private struct ExternalParentResolution {
        let podcastID: UUID
        let shouldHydrateMetadata: Bool
    }

    /// Resolves (or creates a placeholder for) the parent podcast without
    /// hitting the network. The optional feed URL is normalized
    /// case-insensitively to match `store.podcast(feedURL:)`.
    @MainActor
    private func resolveExternalParent(feedURLString: String?) async -> ExternalParentResolution? {
        guard let store else { return nil }
        guard let feedURLString,
              let feedURL = URL(string: feedURLString),
              let scheme = feedURL.scheme?.lowercased(),
              scheme == "http" || scheme == "https" else {
            return ExternalParentResolution(podcastID: Podcast.unknownID, shouldHydrateMetadata: false)
        }
        if let existing = store.podcast(feedURL: feedURL) {
            return ExternalParentResolution(podcastID: existing.id, shouldHydrateMetadata: false)
        }
        // Insert a thin placeholder so the episode has a real parent. Title
        // defaults to the feed host so the UI shows something sensible
        // immediately; metadata hydration overwrites it on success.
        // titleIsPlaceholder stays true until hydration succeeds, letting
        // the UI render it as provisional.
        let placeholder = Podcast(
            kind: .rss,
            feedURL: feedURL,
            title: feedURL.host ?? feedURLString,
            titleIsPlaceholder: true
        )
        let stored = store.upsertPodcast(placeholder)
        return ExternalParentResolution(podcastID: stored.id, shouldHydrateMetadata: true)
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
            kind: .rss,
            feedURL: feedURL,
            title: feedURL.host ?? feedURL.absoluteString,
            titleIsPlaceholder: true
        )
        do {
            let result = try await client.fetch(placeholder)
            if case .updated(var podcast, _, _) = result {
                // Explicit clear — don't rely on FeedClient zero-initialising
                // a field it doesn't know about.
                podcast.titleIsPlaceholder = false
                await MainActor.run {
                    store.updatePodcast(podcast)
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
