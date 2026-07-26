import Foundation

// MARK: - HomeCategoryScope
//
// Pure derivation helpers that narrow Home content to a single category.
// Lifted out of `HomeView` so Continue Listening remains testable without
// a SwiftUI environment or a live store.
//
// All accessors take an optional `allowedSubscriptionIDs: Set<UUID>?`. A
// `nil` value means "no category active — return the global view"; a
// non-`nil` empty set means "category active but contains no shows" and
// every accessor returns an empty result. That asymmetry preserves the
// "All Categories" pseudo-category path the brief explicitly asks us not
// to break.

enum HomeCategoryScope {

    /// Filter `episodes` (which may be the in-progress rail or the recent
    /// feed) by the active category's subscription set. Pass `nil` to
    /// return `episodes` untouched.
    static func episodesInCategory(
        _ episodes: [Episode],
        allowedSubscriptionIDs: Set<UUID>?
    ) -> [Episode] {
        guard let allowed = allowedSubscriptionIDs else { return episodes }
        return episodes.filter { allowed.contains($0.podcastID) }
    }
}
