import Foundation
import os.log

#if canImport(ActivityKit)
import ActivityKit
#endif

/// Translation layer between kernel `PlayerState` transitions (observed in
/// `KernelModel.applyPodcastUpdate` via `reconcileLiveActivity`) and
/// ActivityKit's lifecycle API.
///
/// This is the *executor* half of the Live Activity feature — it never
/// decides *whether* to surface a Live Activity. That decision lives in
/// `KernelModel`, which observes `snapshot.nowPlaying` going non-nil /
/// nil. This class only translates those moments into
/// `Activity<PodcastActivityAttributes>.request(...)`,
/// `activity.update(...)`, and `activity.end(...)` calls.
///
/// Doctrine:
///   D6 — every ActivityKit failure (capability disabled, frequent-update
///        budget exceeded, activity already ended, …) collapses into a
///        log + no-op. The kernel snapshot stays the source of truth;
///        the live activity is a best-effort projection.
///   D7 — no policy lives here. `start` is what `update` becomes after
///        the first call, `stop` is what every `update` becomes after
///        the player clears.
///
/// Threading: every entry point is `@MainActor`. ActivityKit calls are
/// awaited on the main actor; the manager owns no detached tasks.
///
/// Throttling: ActivityKit's frequent-updates budget is ~1 update/sec
/// sustained. We coalesce by rounding `positionSecs` to whole seconds
/// before deciding whether the content state changed. Pause/resume
/// transitions still push through immediately because `isPlaying` is
/// part of the diffed state.
@MainActor
final class LiveActivityManager {

    /// Process-wide instance. The manager is stateless across launches
    /// (ActivityKit owns activity persistence via the system process)
    /// but stateful within a launch — it caches the last pushed
    /// `ContentState` to suppress no-op updates.
    static let shared = LiveActivityManager()

    private static let logger = Logger.app("LiveActivityManager")

    /// Last `ContentState` we pushed to ActivityKit. Used to suppress
    /// duplicate updates so the OS doesn't bill us against the
    /// frequent-updates budget for ticks where nothing changed.
    ///
    /// Erased on `stop()` so the next `start()` is treated as a fresh
    /// activity. Typed as `Any?` because `ContentState` is gated on
    /// `iOS 16.2+`; the cast happens inside `@available` blocks.
    private var lastPushedState: Any?

    /// Episode id of the activity currently in flight, if any. Lets
    /// `start(...)` notice a same-episode call and reduce it to an
    /// `update(...)`, and a different-episode call into an `end → start`.
    private var currentEpisodeID: String?

    private init() {}

    // MARK: - Lifecycle entry points

    /// Begin (or replace) the Live Activity for a now-playing episode.
    /// Idempotent: calling twice with the same episode is treated as an
    /// `update`. Calling with a different episode ends the prior
    /// activity and starts a fresh one — Apple's API does not let a
    /// single activity swap its static `attributes`.
    func start(
        episodeID: String?,
        episodeTitle: String,
        podcastTitle: String,
        positionSecs: Double,
        durationSecs: Double,
        isPlaying: Bool,
        artworkURL: URL?
    ) {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }
        guard ActivityAuthorizationInfo().areActivitiesEnabled else {
            Self.logger.debug("live activities disabled by user; start() no-op")
            return
        }

        // Same episode → degrade to an update so we don't burn an
        // activity-end / activity-request round-trip on every replay
        // of the now-playing observer.
        if let currentEpisodeID, currentEpisodeID == (episodeID ?? "") {
            update(positionSecs: positionSecs, isPlaying: isPlaying)
            return
        }

        // Different episode in flight → wind it down before starting
        // the new one. ActivityKit allows multiple in-flight activities
        // per app, but we deliberately only want one "now playing"
        // surface at a time.
        if currentEpisodeID != nil {
            stop()
        }

        let state = PodcastActivityAttributes.ContentState(
            episodeTitle: episodeTitle,
            podcastTitle: podcastTitle,
            positionSecs: positionSecs,
            durationSecs: durationSecs,
            isPlaying: isPlaying,
            artworkURL: artworkURL)
        let attributes = PodcastActivityAttributes(episodeID: episodeID)

        do {
            // `staleDate: nil` — the system never marks the activity
            // stale on its own. We push fresh content from the kernel
            // snapshot loop; if the loop stops, the lock-screen view's
            // own time-derived position will visibly stop advancing,
            // which is the correct UX cue.
            let activity = try Activity<PodcastActivityAttributes>.request(
                attributes: attributes,
                content: .init(state: state, staleDate: nil),
                pushType: nil)
            currentEpisodeID = episodeID ?? ""
            lastPushedState = state
            Self.logger.info(
                "started live activity id=\(activity.id, privacy: .public) episode=\(episodeID ?? "<nil>", privacy: .public)")
        } catch {
            Self.logger.error("live activity request failed: \(error, privacy: .public)")
        }
        #endif
    }

    /// Push a position / playback-state update to the in-flight activity.
    /// Coalesces by rounding `positionSecs` to whole seconds — back-to-back
    /// ticks within the same second collapse into a single OS update so we
    /// don't exceed ActivityKit's frequent-updates budget.
    func update(positionSecs: Double, isPlaying: Bool) {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }
        guard let previous = lastPushedState as? PodcastActivityAttributes.ContentState,
              Activity<PodcastActivityAttributes>.activities.first != nil
        else { return }

        // 1Hz throttle on position. We still let pause/resume through
        // even when the rounded second hasn't advanced, so transport
        // toggles feel instant on the lock screen.
        let roundedNew = positionSecs.rounded(.down)
        let roundedOld = previous.positionSecs.rounded(.down)
        if roundedNew == roundedOld && previous.isPlaying == isPlaying {
            return
        }

        var next = previous
        next.positionSecs = positionSecs
        next.isPlaying = isPlaying

        // Re-fetch `activity` inside the detached Task — sending an
        // already-bound `Activity` reference across the isolation
        // boundary trips Swift 6 strict-concurrency checking even
        // though `Activity` is `Sendable`. Looking it up from the
        // process-wide `activities` collection avoids the diagnostic
        // and is free (the collection is just a snapshot of the
        // ActivityKit registry).
        Task { [next] in
            guard let activity = Activity<PodcastActivityAttributes>.activities.first else { return }
            await activity.update(.init(state: next, staleDate: nil))
        }
        lastPushedState = next
        #endif
    }

    /// End the in-flight activity (if any). Called when the kernel
    /// snapshot clears `nowPlaying` (episode completed, user stopped
    /// playback, app reset).
    ///
    /// Uses `.dismissalPolicy: .immediate` so the activity disappears
    /// from the lock screen and Dynamic Island as soon as audio stops,
    /// matching the user's mental model: "I pressed stop, so the
    /// ambient surface should go away."
    func stop() {
        #if canImport(ActivityKit)
        guard #available(iOS 16.2, *) else { return }
        let count = Activity<PodcastActivityAttributes>.activities.count
        guard count > 0 else {
            currentEpisodeID = nil
            lastPushedState = nil
            return
        }
        let snapshot = lastPushedState as? PodcastActivityAttributes.ContentState
        // Re-iterate inside the Task — see `update(...)` for the
        // matching strict-concurrency rationale (the `Activity`
        // collection is a snapshot read; recapturing it on the
        // detached side avoids sending an isolated reference).
        Task { [snapshot] in
            let finalState = snapshot.map {
                ActivityContent(state: $0, staleDate: nil)
            }
            for activity in Activity<PodcastActivityAttributes>.activities {
                await activity.end(finalState, dismissalPolicy: .immediate)
            }
        }
        currentEpisodeID = nil
        lastPushedState = nil
        Self.logger.info("ended \(count, privacy: .public) live activity(s)")
        #endif
    }
}
