// KernelModelHashing.swift
// Snapshot-diff helpers that gate `KernelModel.podcastSnapshot` and
// `KernelModel.library` updates. Extracted here to keep KernelModel.swift
// under the 500-line AGENTS.md limit.

// MARK: - KernelModel + snapshot content hashing

extension KernelModel {

    /// Hash of the snapshot fields visible to non-player views. Omits
    /// `nowPlaying.positionSecs`, `nowPlaying.bufferingFraction`, and
    /// `nowPlaying.isBuffering` so views like HomeView and InboxView don't
    /// re-render at 4 Hz during active playback.
    func snapshotContentHash(for update: PodcastUpdate) -> Int {
        var h = Hasher()
        h.combine(update.nowPlaying?.episodeId)
        h.combine(update.nowPlaying?.isPlaying)
        h.combine(update.nowPlaying?.speed)
        h.combine(update.nowPlaying?.durationSecs)
        h.combine(update.nowPlaying?.url)
        h.combine(update.settings.skipForwardSecs)
        h.combine(update.settings.skipBackwardSecs)
        h.combine(update.settings.autoSkipAdsEnabled)
        h.combine(update.settings.hasCompletedOnboarding)
        h.combine(update.toast)
        h.combine(update.activeAccount?.npub)
        h.combine(update.downloads?.active.count)
        h.combine(update.downloads?.queuedCount)
        for d in update.downloads?.active ?? [] { h.combine(d.episodeId); h.combine(d.state) }
        for p in update.picks { h.combine(p.id) }
        for q in update.queue { h.combine(q.id) }
        for i in update.inbox { h.combine(i.id) }
        for t in update.agentTasks { h.combine(t.id); h.combine(t.status) }
        for w in update.wikiArticles { h.combine(w.id) }
        for c in update.clips { h.combine(c.id) }
        for cat in update.categories { h.combine(cat.id); h.combine(cat.episodeCount) }
        for m in update.memoryFacts { h.combine(m.id) }
        for o in update.ownedPodcasts { h.combine(o.id) }
        for s in update.searchResults { h.combine(s.id) }
        for n in update.nostrResults { h.combine(n.id) }
        h.combine(update.agent?.messages.count)
        h.combine(update.agent?.isBusy)
        return h.finalize()
    }

    /// Hash only the fields that list views render. Excludes
    /// `playbackPositionSecs` (and other volatile playback state) so the
    /// `library` property stays stable during active playback.
    func libraryMetaHash(for library: [PodcastSummary]) -> Int {
        var hasher = Hasher()
        for podcast in library {
            hasher.combine(podcast.id)
            hasher.combine(podcast.title)
            hasher.combine(podcast.episodeCount)
            hasher.combine(podcast.artworkUrl)
            hasher.combine(podcast.author)
            for episode in podcast.episodes {
                hasher.combine(episode.id)
                hasher.combine(episode.title)
                hasher.combine(episode.artworkUrl)
                hasher.combine(episode.played)
                hasher.combine(episode.starred)
                hasher.combine(episode.downloadPath)
                hasher.combine(episode.durationSecs)
                hasher.combine(episode.publishedAt)
                for cat in episode.aiCategories {
                    hasher.combine(cat)
                }
            }
        }
        return hasher.finalize()
    }
}
