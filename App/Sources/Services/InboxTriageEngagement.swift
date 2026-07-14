import Foundation

// MARK: - InboxTriageEngagementBuilder
//
// Pure builder that turns `state.episodes` + `state.subscriptions` into
// the per-show `InboxTriageShowEngagement` snapshots the LLM prompt
// consumes. Lives in its own file so `InboxTriageService` stays focused
// on orchestration (candidate selection, LLM call, persistence) and
// the engagement maths are independently readable + testable.

@MainActor
enum InboxTriageEngagementBuilder {

    /// Build engagement snapshots for the given subscriptions.
    ///
    /// - Parameters:
    ///   - store: live `AppStateStore` — read-only access to episodes
    ///     and subscriptions; this builder never mutates state.
    ///   - podcastIDs: shows to snapshot. Typically the parent shows of
    ///     the current triage candidates.
    ///   - showTitles: pre-computed `podcastID -> title` map. Passed in
    ///     so the caller can cache it across builder + candidate-
    ///     selection passes.
    ///   - engagementLookback: how many recent episodes per show to
    ///     consider when counting played / unplayed.
    ///   - newlySubscribedWindow: shows followed within this window
    ///     are tagged `isNewlySubscribed = true`.
    ///   - minSignalEpisodes: safety-net threshold — shows with fewer
    ///     than this many total signals (played + unplayed) are also
    ///     treated as newly subscribed.
    static func build(
        store: AppStateStore,
        podcastIDs: Set<UUID>,
        showTitles: [UUID: String],
        engagementLookback: Int,
        newlySubscribedWindow: TimeInterval,
        minSignalEpisodes: Int
    ) -> [InboxTriageShowEngagement] {
        let subscribedAt = Dictionary(
            uniqueKeysWithValues: store.state.subscriptions.map { ($0.podcastID, $0.subscribedAt) }
        )
        let recentCutoff = Date().addingTimeInterval(-newlySubscribedWindow)
        var snapshots: [InboxTriageShowEngagement] = []
        snapshots.reserveCapacity(podcastIDs.count)

        for podcastID in podcastIDs {
            let episodes = store.state.episodes
                .filter { $0.podcastID == podcastID }
                .sorted { $0.pubDate > $1.pubDate }
                .prefix(engagementLookback)

            var played = 0
            var unplayed = 0
            var lastPlayedAt: Date?
            for ep in episodes {
                if ep.played {
                    played += 1
                    if lastPlayedAt == nil || ep.pubDate > (lastPlayedAt ?? .distantPast) {
                        lastPlayedAt = ep.pubDate
                    }
                } else if !ep.isTriageArchived {
                    // Archived episodes are already routed; counting them
                    // would double-penalise a show the agent itself cleaned up.
                    unplayed += 1
                }
            }

            // Subscription-age check beats the low-signal threshold —
            // long-followed sparse feeds shouldn't get mis-tagged.
            // Zero-signal shows still fall through as a safety net.
            let followedRecently = (subscribedAt[podcastID] ?? .distantPast) >= recentCutoff
            let zeroSignal = (played + unplayed) < minSignalEpisodes
            let isNewlySubscribed = followedRecently || zeroSignal

            snapshots.append(InboxTriageShowEngagement(
                podcastID: podcastID,
                showTitle: showTitles[podcastID] ?? "Unknown show",
                playedCount: played,
                unplayedCount: unplayed,
                lastPlayedAt: lastPlayedAt,
                isNewlySubscribed: isNewlySubscribed
            ))
        }

        // Stable ordering so the prompt is reproducible — sort by
        // descending finish rate so the most-engaged shows surface
        // first (the LLM weights early lines more heavily).
        snapshots.sort { lhs, rhs in
            let lhsRate = lhs.playedCount == 0
                ? 0.0
                : Double(lhs.playedCount) / Double(max(1, lhs.playedCount + lhs.unplayedCount))
            let rhsRate = rhs.playedCount == 0
                ? 0.0
                : Double(rhs.playedCount) / Double(max(1, rhs.playedCount + rhs.unplayedCount))
            return lhsRate > rhsRate
        }
        return snapshots
    }
}
