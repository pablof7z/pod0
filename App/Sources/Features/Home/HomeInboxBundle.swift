import Foundation

// MARK: - HomeInboxBundle
//
// Composes the Home "Inbox" section's hero + secondaries view-model from
// the Rust-owned `PodcastUpdate.inbox` projection. Swift only maps projected
// rows into the existing Home card model and applies the active category
// renderer scope.

@MainActor
enum HomeInboxBundleBuilder {

    /// Max secondaries surfaced below the hero card. Kept stable so layout
    /// assumptions in `HomeFeaturedSection` stay valid.
    static let secondariesCap = 4

    /// Compose the bundle from the store. `allowedSubscriptionIDs` is
    /// the active-category filter (`nil` = All Categories) — when set,
    /// only inbox decisions from podcasts in that subscription set
    /// surface, so the magazine-section UX still works inside Inbox.
    static func make(
        store: AppStateStore,
        allowedSubscriptionIDs: Set<UUID>?,
        now: Date = Date()
    ) -> HomeAgentPicksBundle {
        var picks: [HomeAgentPick] = []
        picks.reserveCapacity(1 + secondariesCap)
        let inbox = store.kernel?.podcastSnapshot?.inbox ?? []
        let scoped = inbox.filter { item in
            guard let podcastID = UUID(uuidString: item.podcastId) else { return false }
            guard let episodeID = UUID(uuidString: item.episodeId),
                  store.episode(id: episodeID) != nil else { return false }
            if let allowed = allowedSubscriptionIDs {
                return allowed.contains(podcastID)
            }
            return true
        }
        for (idx, item) in scoped.prefix(1 + secondariesCap).enumerated() {
            guard let id = UUID(uuidString: item.episodeId) else { continue }
            picks.append(HomeAgentPick(
                episodeID: id,
                rationale: item.priorityReason ?? "",
                spokenRationale: "",
                isHero: idx == 0
            ))
        }

        return HomeAgentPicksBundle(
            picks: picks,
            source: .agent,
            generatedAt: now
        )
    }
}
