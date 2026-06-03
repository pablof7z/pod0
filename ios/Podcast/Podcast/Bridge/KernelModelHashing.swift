// KernelModelHashing.swift
// Snapshot-diff helpers that gate `KernelModel.podcastSnapshot` and
// `KernelModel.library` updates. Extracted here to keep KernelModel.swift
// under the 500-line AGENTS.md limit.

// MARK: - KernelModel + snapshot content hashing

extension KernelModel {

    /// Hash of the snapshot fields visible to non-player views.
    ///
    /// **Excluded** (volatile at 4 Hz during playback, never cause UI change):
    ///   - `nowPlaying.positionSecs`
    ///   - `nowPlaying.bufferingFraction`
    ///   - `nowPlaying.isBuffering`
    ///   - `widget.positionFraction`
    ///   - `library[*].playbackPositionSecs`  (library has its own hash gate)
    ///
    /// Everything else in `PodcastUpdate` is hashed so any real content change
    /// triggers a `podcastSnapshot` update on the next poll.
    func snapshotContentHash(for update: PodcastUpdate) -> Int {
        var h = Hasher()

        // Player state (position excluded — too volatile)
        h.combine(update.nowPlaying?.episodeId)
        h.combine(update.nowPlaying?.isPlaying)
        h.combine(update.nowPlaying?.speed)
        h.combine(update.nowPlaying?.durationSecs)
        h.combine(update.nowPlaying?.url)
        h.combine(update.nowPlaying?.volume)

        // Settings
        h.combine(update.settings.skipForwardSecs)
        h.combine(update.settings.skipBackwardSecs)
        h.combine(update.settings.autoSkipAdsEnabled)
        h.combine(update.settings.hasCompletedOnboarding)

        // Misc
        h.combine(update.toast)
        h.combine(update.activeAccount?.npub)

        // Downloads (state, not progress)
        h.combine(update.downloads?.active.count)
        h.combine(update.downloads?.queuedCount)
        h.combine(update.downloads?.completedToday)
        for d in update.downloads?.active ?? [] {
            h.combine(d.episodeId)
            h.combine(d.state)
            // d.progress excluded (volatile, changes during download)
        }

        // Picks, queue, inbox, tasks
        for p in update.picks { h.combine(p.id) }
        for q in update.queue { h.combine(q.id) }
        for i in update.inbox { h.combine(i.id) }
        for t in update.agentTasks { h.combine(t.id); h.combine(t.status) }

        // Wiki / knowledge
        for w in update.wikiArticles { h.combine(w.id); h.combine(w.isGenerating) }
        for w in update.wikiSearchResults { h.combine(w.id) }
        for k in update.knowledgeSearchResults { h.combine(k.id) }

        // Categories (include topEpisodeIds — their order/content can change)
        for cat in update.categories {
            h.combine(cat.id)
            h.combine(cat.episodeCount)
            for epId in cat.topEpisodeIds { h.combine(epId) }
        }

        // Memory, TTS, clips
        for m in update.memoryFacts { h.combine(m.id); h.combine(m.value) }
        for t in update.ttsEpisodes { h.combine(t.id); h.combine(t.status) }
        for c in update.clips { h.combine(c.id) }

        // Ownership, search
        for o in update.ownedPodcasts { h.combine(o.id) }
        for s in update.searchResults { h.combine(s.id) }
        for n in update.nostrResults { h.combine(n.id) }

        // Comments
        for c in update.comments { h.combine(c.id) }

        // Agent
        h.combine(update.agent?.messages.count)
        h.combine(update.agent?.isBusy)

        // Voice (state transitions that matter for UI)
        h.combine(update.voice?.isSpeaking)
        h.combine(update.voice?.isListening)
        h.combine(update.voice?.currentRequestId)
        h.combine(update.voice?.lastResponse)

        // Social
        h.combine(update.social?.followingCount)

        // Widget (positionFraction excluded — too volatile)
        h.combine(update.widget?.nowPlayingEpisodeTitle)
        h.combine(update.widget?.nowPlayingPodcastTitle)
        h.combine(update.widget?.isPlaying)
        h.combine(update.widget?.unplayedCount)

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
