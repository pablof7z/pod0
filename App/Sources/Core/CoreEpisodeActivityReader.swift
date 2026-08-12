import Pod0Core

/// Read-only bridge to the redacted Rust journal projection. It owns no state
/// and performs no interpretation of activity facts.
actor CoreEpisodeActivityReader {
    static let shared = CoreEpisodeActivityReader()

    func page(
        for episodeID: EpisodeId,
        from facade: Pod0Facade,
        maximumCount: UInt16 = 200
    ) -> EpisodeActivityPage {
        var after: UInt64?
        var items: [EpisodeActivityEntry] = []
        repeat {
            let page = facade.episodeActivityPage(
                episodeId: episodeID,
                afterSequence: after,
                requestedCount: maximumCount
            )
            guard page.available else { return page }
            items.append(contentsOf: page.items)
            after = page.nextAfterSequence
        } while after != nil
        return EpisodeActivityPage(
            available: true,
            items: items,
            nextAfterSequence: nil
        )
    }
}
