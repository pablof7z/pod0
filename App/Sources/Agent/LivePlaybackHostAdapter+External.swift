import Foundation
import Pod0Core

extension LivePlaybackHostAdapter {
    func playExternalEpisode(
        audioURL: URL,
        title: String,
        feedURLString: String?,
        durationSeconds: TimeInterval?,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition
    ) async -> PlayEpisodeResult? {
        guard let parent = await resolveExternalParent(feedURLString: feedURLString),
              let store else {
            logger.error("playExternalEpisode: store unavailable")
            return nil
        }
        let feedURL = feedURLString.flatMap(URL.init(string:))
        let podcastTitle = await store.podcast(id: parent.podcastID)?.title
            ?? feedURL?.host
            ?? "Unknown Podcast"
        let episode: Episode
        do {
            episode = try await store.upsertExternalEpisodeAndWait(
                podcastID: parent.podcastID,
                feedURL: feedURL,
                podcastTitle: podcastTitle,
                audioURL: audioURL,
                title: title,
                imageURL: nil,
                duration: durationSeconds
            )
        } catch {
            logger.error("playExternalEpisode: durable insert failed: \(error.localizedDescription, privacy: .public)")
            return nil
        }
        let result = await play(
            episode,
            title: title,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            queuePosition: queuePosition,
            store: store
        )
        if let feedURL,
           parent.shouldHydrateMetadata {
            Task.detached { [weak self] in
                await self?.hydratePlaceholderPodcastMetadata(
                    podcastID: parent.podcastID,
                    feedURL: feedURL
                )
            }
        }
        return result
    }

    @MainActor
    private func play(
        _ episode: Episode,
        title: String,
        startSeconds: Double?,
        endSeconds: Double?,
        queuePosition: QueuePosition,
        store: AppStateStore
    ) -> PlayEpisodeResult? {
        guard let playback else { return nil }
        let item = QueueItem(
            episodeID: episode.id,
            startSeconds: startSeconds,
            endSeconds: endSeconds,
            label: nil
        )
        let startedPlaying: Bool
        switch queuePosition {
        case .now:
            playback.enqueueSegments([item], playNow: true) { store.episode(id: $0) }
            startedPlaying = true
        case .next:
            playback.insertNext(item)
            startedPlaying = false
        case .end:
            playback.enqueueItem(item)
            startedPlaying = false
        }
        logger.info("playExternalEpisode: '\(title, privacy: .public)' queued at \(String(describing: queuePosition), privacy: .public)")
        return PlayEpisodeResult(
            episodeID: episode.id.uuidString,
            queuePosition: queuePosition,
            startedPlaying: startedPlaying,
            episodeTitle: episode.title,
            podcastTitle: store.podcast(id: episode.podcastID)?.title,
            durationSeconds: episode.duration.map { Int($0) }
        )
    }

    @MainActor
    private func resolveExternalParent(feedURLString: String?) -> ExternalParentResolution? {
        guard let store else { return nil }
        guard let feedURLString,
              let feedURL = URL(string: feedURLString),
              ["http", "https"].contains(feedURL.scheme?.lowercased() ?? "") else {
            return ExternalParentResolution(
                podcastID: Podcast.unknownID,
                shouldHydrateMetadata: false
            )
        }
        if let existing = store.podcast(feedURL: feedURL) {
            return ExternalParentResolution(
                podcastID: existing.id,
                shouldHydrateMetadata: false
            )
        }
        let placeholder = Podcast(
            kind: .rss,
            feedURL: feedURL,
            title: feedURL.host ?? feedURLString,
            titleIsPlaceholder: true
        )
        return ExternalParentResolution(
            podcastID: store.upsertPodcast(placeholder).id,
            shouldHydrateMetadata: true
        )
    }

    private func hydratePlaceholderPodcastMetadata(podcastID: UUID, feedURL: URL) async {
        guard let store else { return }
        if await store.isSharedLibraryAuthoritative {
            do {
                guard let sharedLibrary = await store.sharedLibrary else {
                    throw SharedLibraryError.unavailable
                }
                _ = try await sharedLibrary.execute(.hydratePodcastMetadata(
                    podcastId: PodcastId(uuid: podcastID)
                ))
            } catch {
                logger.error("playExternalEpisode: shared metadata hydration failed: \(error.localizedDescription, privacy: .public)")
            }
            return
        }
        let placeholder = Podcast(
            id: podcastID,
            feedURL: feedURL,
            title: feedURL.host ?? feedURL.absoluteString,
            titleIsPlaceholder: true
        )
        do {
            if case .updated(var podcast, _, _) = try await FeedClient().fetch(placeholder) {
                podcast.titleIsPlaceholder = false
                await store.updatePodcast(podcast)
            }
        } catch {
            logger.error("playExternalEpisode: metadata fetch failed: \(error.localizedDescription, privacy: .public)")
        }
    }

    private struct ExternalParentResolution {
        let podcastID: UUID
        let shouldHydrateMetadata: Bool
    }
}
