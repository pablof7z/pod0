import Pod0Core

extension SharedLibraryClient {
    func receiveLibrary(_ envelope: ProjectionEnvelope) {
        guard envelope.stateRevision.value >= lastLibraryRevision else { return }
        lastLibraryRevision = envelope.stateRevision.value
        let previous = cachedSnapshot
        let snapshot = loadAllPages()
        let readModelChanged = previous.map { !$0.hasSameReadModel(as: snapshot) } ?? true
        cachedSnapshot = snapshot
        if readModelChanged {
            store?.applySharedLibrary(snapshot)
            announcePublisherSourceChanges(previous: previous, current: snapshot)
        }
        resolveWaiters(snapshot.operations)
        dispatcher.executePendingRequests(from: facade)
        if readModelChanged {
            WorkflowRuntime.shared.wake()
        }
    }

    func loadAllPages() -> SharedLibrarySnapshot {
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
        let activeEpisodeIDs = Set(chapterScopeCounts.keys)
        chapterSnapshots = Dictionary(uniqueKeysWithValues: activeEpisodeIDs.compactMap {
            episodeID in
            guard let snapshot = try? authoritativeChapterReader.load(episodeID: episodeID)
            else { return nil }
            return (episodeID, snapshot)
        })
        return SharedLibrarySnapshot(
            podcasts: podcasts,
            subscriptions: subscriptions,
            episodes: episodes,
            chaptersByEpisodeID: chapterSnapshots,
            operations: operations
        )
    }
}
