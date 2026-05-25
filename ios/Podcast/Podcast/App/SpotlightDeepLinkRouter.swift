import Foundation
import Observation

// MARK: - SpotlightDeepLinkRouter
//
// Bridges Spotlight tap activities into the SwiftUI navigation graph.
//
// Flow:
//   1. The OS delivers a `CSSearchableItemActionType` `NSUserActivity`
//      to `PodcastApp.onContinueUserActivity`.
//   2. `PodcastApp` decodes it via `SpotlightCapability.deepLink(...)`
//      and stashes the resulting case on `pendingDeepLink`.
//   3. `RootShell` observes `pendingDeepLink` to flip its active tab
//      to `.library` (Spotlight only indexes library content today).
//   4. `LibraryView` observes `pendingDeepLink` and, once
//      `model.library` contains the referenced row, pushes the
//      appropriate route onto its `NavigationStack` and calls
//      `consume()` to clear the slot.
//
// Cold-start: the activity often arrives before the snapshot poll
// has populated `model.library`. The router holds the deep link
// until the consumer can satisfy it, instead of dropping it on the
// floor. `LibraryView` also re-evaluates on `model.library` changes
// so a snapshot landing after the activity completes the deep link
// retroactively.
//
// Single-shot: `pendingDeepLink` is a one-slot mailbox, not a queue.
// If two activities arrive before the first is consumed (vanishingly
// unlikely for human-driven Spotlight taps), the newer one wins.

@MainActor
@Observable
final class SpotlightDeepLinkRouter {

    /// The deep link waiting to be consumed by the navigation stack.
    /// Cleared by `consume()`; replaced wholesale by `requestNavigation(_:)`.
    private(set) var pendingDeepLink: SpotlightCapability.DeepLink?

    init() {}

    /// Record a Spotlight tap. The new deep link replaces any unconsumed
    /// previous one — see the "single-shot" note above.
    func requestNavigation(to deepLink: SpotlightCapability.DeepLink) {
        pendingDeepLink = deepLink
    }

    /// Convenience for the `.onContinueUserActivity` callback in
    /// `PodcastApp`. Returns true when the activity was a Spotlight
    /// tap we recognised (and stashed); false otherwise (so the caller
    /// can chain another handler if more activity types are added).
    @discardableResult
    func handle(_ activity: NSUserActivity) -> Bool {
        guard let deepLink = SpotlightCapability.deepLink(fromActivity: activity) else {
            return false
        }
        requestNavigation(to: deepLink)
        return true
    }

    /// Clear the pending slot after a consumer has navigated to it.
    /// Idempotent — calling on an empty slot is a no-op.
    func consume() {
        pendingDeepLink = nil
    }
}
