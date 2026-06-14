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
    ///   - `widget.positionFraction`
    ///   - `library[*].playbackPositionSecs`  (library has its own hash gate)
    ///
    /// Everything else in `PodcastUpdate` is hashed so any real content change
    /// triggers a `podcastSnapshot` update on the next pushed/pulled frame.
    ///
    /// `nonisolated`: reads only the `update` parameter and a local `Hasher`,
    /// never `self`, so it is safe to run off the MainActor. `applyPodcastUpdate`
    /// offloads this computation to a background `Task.detached` so the hash
    /// cost is no longer paid on the MainActor at the 4 Hz emit rate.
    nonisolated func snapshotContentHash(for update: PodcastUpdate) -> Int {
        var h = Hasher()

        // Player state (position excluded — too volatile)
        h.combine(update.nowPlaying?.episodeId)
        h.combine(update.nowPlaying?.isPlaying)
        h.combine(update.nowPlaying?.speed)
        h.combine(update.nowPlaying?.durationSecs)
        h.combine(update.nowPlaying?.url)
        h.combine(update.nowPlaying?.volume)
        h.combine(update.nowPlaying?.currentChapterTitle)

        // Settings
        h.combine(update.settings.skipForwardSecs)
        h.combine(update.settings.skipBackwardSecs)
        h.combine(update.settings.autoSkipAdsEnabled)
        h.combine(update.settings.hasCompletedOnboarding)

        // Misc
        h.combine(update.toast)
        h.combine(update.activeAccount?.pubkeyHex)
        h.combine(update.activeAccount?.fingerprint)

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
        h.combine(update.inboxLastTriagedAt)
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

        // Agent-prompt inventory context. The kernel derives this from
        // library fields (playback position, played, triage) that are
        // deliberately EXCLUDED from this hash because their raw values are
        // volatile. But `agentContext` carries only titles + counts — its
        // *membership* changes solely when an episode enters/leaves the
        // in-progress or recent-unplayed set (or a sub is added/removed), not
        // on every position tick. Hashing it here keeps `podcastSnapshot`
        // (and therefore `AgentPrompt`) fresh when that membership shifts,
        // without reintroducing per-tick republish churn.
        if let ctx = update.agentContext {
            h.combine(ctx.subscriptionsTotal)
            for title in ctx.subscriptions { h.combine(title) }
            for ep in ctx.inProgress { h.combine(ep.title); h.combine(ep.showTitle) }
            for ep in ctx.recentUnplayed { h.combine(ep.title); h.combine(ep.showTitle) }
        }

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
    ///
    /// `nonisolated`: reads only the `library` parameter and a local `Hasher`,
    /// never `self`. This is the O(N×M) cost (every show × every episode ×
    /// multiple fields) that `applyPodcastUpdate` now runs off the MainActor.
    nonisolated func libraryMetaHash(for library: [PodcastSummary]) -> Int {
        var hasher = Hasher()
        for podcast in library {
            hasher.combine(podcast.id)
            hasher.combine(podcast.title)
            hasher.combine(podcast.episodeCount)
            hasher.combine(podcast.isSubscribed)
            hasher.combine(podcast.artworkUrl)
            hasher.combine(podcast.author)
            // A feed-host→real-title hydration of an external-play placeholder
            // changes `feedUrl`/`title`; include feedUrl so the projection
            // refreshes when only the feed URL is enriched.
            hasher.combine(podcast.feedUrl)
            hasher.combine(podcast.lastRefreshedAt)
            hasher.combine(podcast.titleIsPlaceholder)
            // Owned-podcast identity: a visibility flip or ownership claim
            // mutates these without touching title/artwork, so include them
            // or the library projection would not refresh on an owned-podcast
            // update.
            hasher.combine(podcast.ownerPubkeyHex)
            hasher.combine(podcast.nostrVisibility)
            for episode in podcast.episodes {
                hasher.combine(episode.id)
                hasher.combine(episode.title)
                hasher.combine(episode.artworkUrl)
                hasher.combine(episode.played)
                hasher.combine(episode.starred)
                hasher.combine(episode.downloadPath)
                // Lifecycle-locked to `downloadPath`, but hashed too so a
                // re-download to the same path with a different size still
                // bumps the library hash and refreshes the displayed size.
                hasher.combine(episode.fileSizeBytes)
                hasher.combine(episode.durationSecs)
                hasher.combine(episode.publishedAt)
                // Include `summary` so a freshly-landed AI summary changes the
                // library hash and the `summarize_episode` tool's snapshot await
                // is actually woken (without this, the rev bump is deduped).
                hasher.combine(episode.summary)
                for cat in episode.aiCategories {
                    hasher.combine(cat)
                }
            }
        }
        return hasher.finalize()
    }
}
