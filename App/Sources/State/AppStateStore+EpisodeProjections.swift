import Foundation

// MARK: - AppStateStore + EpisodeProjections
//
// **Why this exists.** Several SwiftUI surfaces — the Library subscriptions
// grid, ShowDetail, Home Today's Continue Listening + New Episodes feeds —
// read derived facts about `state.episodes`. Each fact is O(N) to compute
// (filter / sort / contains). Each fact is read inside a `body` getter that
// SwiftUI re-runs every scroll tick, every cell. With 20 subscriptions and
// 10k episodes, scrolling Library used to fire 20 × 10k = 200k filter
// iterations *per scroll tick*. The `unplayedCount(forPodcast:)` frame
// alone showed up at 27 ticks in `sample` — the call graph was dominated by
// per-cell helpers iterating the entire episode array and copying full
// `Episode` structs.
//
// The fix: precompute every O(N) fact once, off `state.episodes`, into a
// dictionary or set keyed by subscription ID (or, for the Home feeds, into
// a flat sorted array). Reads become O(1) dict lookups, the per-cell
// helpers become trivial.
//
// **Cache shape.** Stored on `AppStateStore` itself (extensions can't add
// stored state); see `AppStateStore.swift` for the property declarations.
//
// **Invalidation.** Every method in `AppStateStore+Episodes.swift` that
// mutates `state.episodes` calls `invalidateEpisodeProjections()` after
// the mutation lands. As a safety net, `state.didSet` also recomputes
// when the array fingerprint changes — covering bulk assignments
// (`clearAllData`, persistence reload) and the one episode-removing path
// in `removeSubscription` over in `+Podcasts`.
//
// **Position-cache fold.** `inProgressEpisodes` and `recentEpisodes`
// surface live playhead values via the position-debounce cache (see
// `AppStateStore+PositionDebounce.swift`). The cache is folded into the
// returned values *at read time*, not stored in the precomputed array,
// so a 1 Hz playback tick doesn't dirty the projection.
//
// **Memory.** Per-show projections store sorted indexes into `state.episodes`,
// not full `Episode` copies. Reads materialize just the visible show's slice,
// keeping the large imported-library footprint lower than a duplicate cache.

extension AppStateStore {

    // MARK: - Public invalidation entry points

    /// Rebuilds every cached projection from the current `state.episodes`.
    /// Cheap relative to the per-cell helpers it replaces (one O(N) pass
    /// instead of N × number-of-cells per scroll tick), but still not free
    /// — callers should fire it at most once per logical mutation, not in
    /// a hot loop.
    ///
    /// Internal API surface: callers in `AppStateStore+Episodes.swift`,
    /// `+Podcasts.swift`, and `+PositionDebounce.swift` invoke this after
    /// any write that affects unplayed counts, download states, transcript
    /// states, episode membership, or position-derived in-progress status.
    func recomputeEpisodeProjections() {
        let episodes = state.episodes

        var unplayed: [UUID: Int] = [:]
        var downloaded: Set<UUID> = []
        var transcribed: Set<UUID> = []
        var byShow: [UUID: [Int]] = [:]
        var inProgress: [Episode] = []
        var recent: [Episode] = []

        // Reserve capacity to avoid the rehash storm when growing through
        // a 10k-episode pass. Conservative bounds: bucket counts ≤ unique
        // subscriptions; per-show episode arrays ≤ episodes / shows. We
        // can't know either without a first pass, so reserve to the high
        // end of the cache itself.
        unplayed.reserveCapacity(state.subscriptions.count)
        downloaded.reserveCapacity(state.subscriptions.count)
        transcribed.reserveCapacity(state.subscriptions.count)
        byShow.reserveCapacity(state.subscriptions.count)
        inProgress.reserveCapacity(min(64, episodes.count))
        recent.reserveCapacity(min(Self.recentEpisodesCacheLimit, episodes.count))

        for (index, episode) in episodes.enumerated() {
            let podID = episode.podcastID

            // Unplayed-count bucket. Default to 0 so the dict has an entry
            // for every show that has any episode at all (cheaper than
            // checking `contains` on read). AI Inbox: silently-archived
            // episodes (triageDecision == .archived) drop out of the count
            // so the dot indicator and "X unplayed" labels reflect what the
            // user is actually expected to hear, not the noisy raw feed.
            if !episode.played && !episode.isTriageArchived {
                unplayed[podID, default: 0] += 1
            } else if unplayed[podID] == nil {
                // Ensure the show has *some* entry so reads default to 0
                // without falling through to a non-existent key.
                unplayed[podID] = 0
            }

            // Downloaded / transcribed presence. Cheap to set repeatedly
            // for the same podID; Set.insert is O(1) amortised.
            if case .downloaded = episode.downloadState {
                downloaded.insert(podID)
            }
            if case .ready = episode.transcriptState {
                transcribed.insert(podID)
            }

            // Per-show index cache: append indexes now, sort once per show.
            byShow[podID, default: []].append(index)

            // In-progress: persisted position > 0 AND not played AND not
            // triage-archived. The position-cache fold at read time also
            // surfaces episodes whose cached position crossed zero but
            // haven't been persisted yet (`inProgressEpisodesView`).
            // Archived check is defensive — if the user already started an
            // episode the triage pass wouldn't have archived it, but a
            // user-initiated archive (future) shouldn't keep it on the
            // Continue Listening rail either.
            if !episode.played, episode.playbackPosition > 0, !episode.isTriageArchived {
                inProgress.append(episode)
            }
        }

        // Sort each per-show array newest-pubDate-first. Total cost:
        // sum of N_i log N_i across shows ≤ N log N overall — same big-O
        // as a single global sort but with much smaller per-bucket work.
        // Mutate values in place via key iteration so the dict's COW
        // buffer isn't reseated for every show.
        for id in byShow.keys {
            byShow[id]?.sort { episodes[$0].pubDate > episodes[$1].pubDate }
        }

        inProgress.sort { $0.pubDate > $1.pubDate }

        // recentEpisodesCached: top-N unplayed episodes across all shows.
        // We do a global sort + prefix here. For 10k episodes this is
        // still cheap (single 10k sort, no allocation per cell).
        // AI Inbox: triage-archived episodes are silently dismissed and
        // must not appear in the recent feed either, otherwise the agent's
        // archive decision has no effect on the surfaces it was meant to
        // de-clutter.
        recent = episodes.indices
            .lazy
            .filter { !episodes[$0].played && !episodes[$0].isTriageArchived }
            .sorted { episodes[$0].pubDate > episodes[$1].pubDate }
            .prefix(Self.recentEpisodesCacheLimit)
            .map { episodes[$0] }

        unplayedCountByShow = unplayed
        hasDownloadedByShow = downloaded
        hasTranscribedByShow = transcribed
        episodeIndexesByShow = byShow
        inProgressEpisodesCached = inProgress
        recentEpisodesCached = recent
    }

    /// Alias for `recomputeEpisodeProjections()`. Kept as a separate name
    /// so call sites read intent-fully — `setEpisodeDownloadState` calls
    /// `invalidateEpisodeProjections()` to mean "the cached download/
    /// transcribed sets may now be stale", not "rebuild everything for
    /// performance reasons".
    func invalidateEpisodeProjections() {
        markEpisodeProjectionsDirty()
    }

    // MARK: - Read-side helpers (position-cache fold)

    /// `inProgressEpisodes`-shaped view, with the position-debounce cache
    /// folded in.
    ///
    /// Two cases the read needs to handle correctly:
    ///   1. **Persisted > 0, cache absent.** Episode is in the cached
    ///      `inProgressEpisodesCached` list as-is.
    ///   2. **Persisted == 0, cache > 0.** Engine just started ticking but
    ///      hasn't flushed yet. Episode is NOT in the cached list and must
    ///      be appended below.
    ///   3. **Persisted > 0, cache == 0.** Engine wrote a zero (e.g. user
    ///      scrubbed to the very start). The cached list still includes the
    ///      episode but the post-fold position is 0 — the `> 0` filter
    ///      after `applyingPositionCache` drops it so the Continue
    ///      Listening rail doesn't show a stale entry.
    ///
    /// The third case is what the trailing `.filter { ... > 0 }` defends
    /// against. Without it, `applyingPositionCache` would overwrite the
    /// position with the cached 0 and leave a phantom in-progress entry.
    func inProgressEpisodesView() -> [Episode] {
        var result = applyingPositionCache(inProgressEpisodesCached)
            .filter { $0.playbackPosition > 0 }

        // Case 2: episode whose persisted position is still 0 but whose
        // cache has crossed > 0 must surface here. `positionCache` is
        // typically tiny — at most a handful of episodes the engine has
        // ticked since the last flush.
        if !positionCache.isEmpty {
            let existingIDs = Set(inProgressEpisodesCached.map(\.id))
            for (id, position) in positionCache where position > 0 && !existingIDs.contains(id) {
                guard var ep = state.episodes.first(where: { $0.id == id }), !ep.played else { continue }
                ep.playbackPosition = position
                result.append(ep)
            }
            // Re-sort if we appended; preserve newest-first ordering.
            result.sort { $0.pubDate > $1.pubDate }
        }

        return result
    }

    /// `recentEpisodes(limit:)`-shaped view, folding in the position
    /// cache so a freshly-started episode reads with its live playhead.
    /// Honours `limit` against the cached top-N; if a caller requests
    /// more than `recentEpisodesCacheLimit`, falls back to a full
    /// recompute against `state.episodes`.
    func recentEpisodesView(limit: Int) -> [Episode] {
        if limit <= Self.recentEpisodesCacheLimit {
            return applyingPositionCache(Array(recentEpisodesCached.prefix(limit)))
        }
        // Rare cold path: caller asked for more than we cache.
        let recomputed = state.episodes
            .lazy
            .filter { !$0.played }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(limit)
            .map { $0 }
        return applyingPositionCache(Array(recomputed))
    }

    /// Pre-sorted, position-cache-folded list of episodes for one show.
    /// Backed by `episodeIndexesByShow`; no per-call filter or sort.
    func episodesForShowView(_ id: UUID) -> [Episode] {
        guard let indexes = episodeIndexesByShow[id] else { return [] }
        let episodes = state.episodes
        let cached = indexes.compactMap { index -> Episode? in
            guard episodes.indices.contains(index) else { return nil }
            return episodes[index]
        }
        return applyingPositionCache(cached)
    }

    // MARK: - Fingerprint (didSet safety net)

    /// Cheap inequality check used by `state.didSet` to decide whether
    /// the episode projections need recomputing. Compares `count` and
    /// first/last `id` only — purely a safety net for paths that replace
    /// the entire array (`clearAllData`, persistence reload). Per-element
    /// edits inside `state.episodes` (e.g. `state.episodes[idx].played =
    /// true`) that don't change array shape would be missed here, so
    /// every dedicated writer also calls `invalidateEpisodeProjections()`
    /// after the mutation lands. The two paths together guarantee no
    /// stale-cache reads.
    static func episodesFingerprintChanged(_ lhs: [Episode], _ rhs: [Episode]) -> Bool {
        if lhs.count != rhs.count { return true }
        guard !lhs.isEmpty else { return false }
        if lhs.first?.id != rhs.first?.id { return true }
        if lhs.last?.id != rhs.last?.id { return true }
        return false
    }
}
