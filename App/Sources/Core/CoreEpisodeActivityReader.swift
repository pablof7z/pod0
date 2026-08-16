import Pod0Core

/// Read-only bridge to the redacted Rust journal projection. It owns no state
/// and performs no interpretation of activity facts.
actor CoreEpisodeActivityReader {
    static let shared = CoreEpisodeActivityReader()
    static let defaultPageSize: UInt16 = 40
    static let maximumPageSize: UInt16 = 100

    func firstPage(
        for episodeID: EpisodeId,
        from facade: Pod0Facade?,
        requestedCount: UInt16 = defaultPageSize
    ) -> LatestEpisodeActivityPage {
        guard let facade else { return Self.unavailablePage }
        return facade.latestEpisodeActivityPage(
            episodeId: episodeID,
            snapshotThroughSequence: nil,
            beforeSequence: nil,
            requestedCount: Self.bounded(requestedCount)
        )
    }

    func loadMore(
        for episodeID: EpisodeId,
        current: LatestEpisodeActivityPage,
        from facade: Pod0Facade,
        requestedCount: UInt16 = defaultPageSize
    ) -> LatestEpisodeActivityPage {
        guard current.available,
              let snapshot = current.snapshotThroughSequence,
              let before = current.nextBeforeSequence else { return current }
        let next = facade.latestEpisodeActivityPage(
            episodeId: episodeID,
            snapshotThroughSequence: snapshot,
            beforeSequence: before,
            requestedCount: Self.bounded(requestedCount)
        )
        guard next.available,
              next.snapshotThroughSequence == snapshot else { return current }
        return LatestEpisodeActivityPage(
            available: true,
            items: current.items + next.items,
            snapshotThroughSequence: snapshot,
            nextBeforeSequence: next.nextBeforeSequence
        )
    }

    private static func bounded(_ requestedCount: UInt16) -> UInt16 {
        min(max(1, requestedCount), maximumPageSize)
    }

    private static var unavailablePage: LatestEpisodeActivityPage {
        LatestEpisodeActivityPage(
            available: false,
            items: [],
            snapshotThroughSequence: nil,
            nextBeforeSequence: nil
        )
    }
}
