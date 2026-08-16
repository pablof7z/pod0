import Foundation
import Pod0Core

extension SharedLibraryClient {
    func receiveLibrary(_ envelope: ProjectionEnvelope) {
        guard envelope.stateRevision.value >= lastLibraryRevision else { return }
        lastLibraryRevision = envelope.stateRevision.value
        guard case .library(let projection) = envelope.projection else { return }
        guard envelope.contentChanged else {
            resolveWaiters(projection.operations)
            return
        }
        let previous = cachedSnapshot
        let revision = envelope.stateRevision.value
        let facade = facade
        let activeEpisodeIDs = chapterScopes.retainedEpisodeIDs
        let chapterReader = authoritativeChapterReader
        libraryProjectionTask?.cancel()
        libraryProjectionTask = Task { @MainActor [weak self] in
            let snapshot = await Task.detached(priority: .utility) {
                Self.loadAllPages(
                    facade: facade,
                    activeEpisodeIDs: activeEpisodeIDs,
                    chapterReader: chapterReader
                )
            }.value
            guard !Task.isCancelled, let self, revision <= lastLibraryRevision else {
                return
            }
            chapterSnapshots = snapshot.chaptersByEpisodeID
            let changed = previous.map { !$0.hasSameReadModel(as: snapshot) } ?? true
            cachedSnapshot = snapshot
            if changed {
                store?.applySharedLibrary(snapshot)
                WorkflowRuntime.shared.wake()
            }
            resolveWaiters(projection.operations)
        }
    }

    nonisolated static func loadAllPages(
        facade: Pod0Facade,
        activeEpisodeIDs: Set<UUID>,
        chapterReader: SharedChapterReader
    ) -> SharedLibrarySnapshot {
        var offset: UInt32 = 0
        var podcasts: [PodcastRecord] = []
        var subscriptions: [PodcastSubscriptionRecord] = []
        var episodes: [EpisodeRecord] = []
        var feedFetches: [FeedFetchProjection] = []
        var operations: [OperationProjection] = []
        while true {
            let envelope = facade.snapshot(request: ProjectionRequest(
                scope: .library,
                offset: offset,
                maxItems: 200
            ))
            guard case .library(let page) = envelope.projection else { break }
            podcasts.append(contentsOf: page.podcasts)
            subscriptions.append(contentsOf: page.subscriptions)
            episodes.append(contentsOf: page.episodes)
            if operations.isEmpty { operations = page.operations }
            if feedFetches.isEmpty { feedFetches = page.feedFetches }
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
        EpisodeShowNotesFormatter.prewarm(
            episodes.prefix(1_000).map(\.description)
        )
        let chapters: [UUID: SharedChapterSnapshot] = Dictionary(
            uniqueKeysWithValues: activeEpisodeIDs.compactMap {
            episodeID in
            guard let snapshot = try? chapterReader.load(episodeID: episodeID)
            else { return nil }
            return (episodeID, snapshot)
            }
        )
        return SharedLibrarySnapshot(
            podcasts: podcasts,
            subscriptions: subscriptions,
            episodes: episodes,
            feedFetches: feedFetches,
            chaptersByEpisodeID: chapters,
            operations: operations
        )
    }
}
