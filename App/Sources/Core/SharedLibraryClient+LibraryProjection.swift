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
        let activeEpisodeIDs = Set(chapterScopeCounts.keys)
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
                announcePublisherSourceChanges(previous: previous, current: snapshot)
                WorkflowRuntime.shared.wake()
            }
            resolveWaiters(projection.operations)
        }
    }

    func loadAllPages() -> SharedLibrarySnapshot {
        let snapshot = Self.loadAllPages(
            facade: facade,
            activeEpisodeIDs: Set(chapterScopeCounts.keys),
            chapterReader: authoritativeChapterReader
        )
        chapterSnapshots = snapshot.chaptersByEpisodeID
        return snapshot
    }

    nonisolated private static func loadAllPages(
        facade: Pod0Facade,
        activeEpisodeIDs: Set<UUID>,
        chapterReader: SharedChapterReader
    ) -> SharedLibrarySnapshot {
        var offset: UInt32 = 0
        var podcasts: [PodcastRecord] = []
        var subscriptions: [PodcastSubscriptionRecord] = []
        var episodes: [EpisodeRecord] = []
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
            guard page.hasMore, offset <= UInt32.max - 200 else { break }
            offset += 200
        }
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
            chaptersByEpisodeID: chapters,
            operations: operations
        )
    }
}
