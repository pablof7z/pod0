import Foundation

#if canImport(ActivityKit)
import ActivityKit

/// Wire contract for the podcast Live Activity.
///
/// Shared between the main `Podcast` app target (which calls
/// `Activity<PodcastActivityAttributes>.request(...)` to start the
/// activity, and `activity.update(...)` to push state changes) and the
/// `PodcastWidget` extension target (which renders
/// `ActivityConfiguration<PodcastActivityAttributes>` for the lock screen
/// and Dynamic Island).
///
/// The two targets compile separately, so we share *this file* — listed
/// under both targets' `sources:` in `ios/Podcast/project.yml` — rather
/// than rely on a Swift module dependency. The widget extension cannot
/// link the main app's symbols; sharing the source file is the standard
/// ActivityKit pattern Apple documents for Live Activities.
///
/// Doctrine:
///   D6 — `ContentState` is `Codable`/`Hashable` so the system can
///        diff and persist activity state across app launches and
///        device locks without us writing custom serialization.
///   D7 — this type is wire-only; it does not decide *when* to start,
///        update, or stop an activity. The kernel (via `KernelModel`
///        snapshot observation) picks the lifecycle moments; the
///        executor (`LiveActivityManager`) only translates the kernel's
///        decision into ActivityKit calls.
///
/// iOS 16.2+ only. The whole file is gated on `canImport(ActivityKit)`
/// so the type is *absent* from older SDKs / non-iOS targets (Android
/// shell etc.) — callers must use `if #available(iOS 16.2, *)` at every
/// use site even though the project deployment target is iOS 26.0.
@available(iOS 16.2, *)
struct PodcastActivityAttributes: ActivityAttributes {

    /// Mutable per-update payload. Position + isPlaying change on every
    /// tick; episode/podcast/artwork are immutable for the lifetime of
    /// a given activity instance (a new episode starts a new activity).
    public struct ContentState: Codable, Hashable {
        /// Title of the episode currently in the player.
        public var episodeTitle: String
        /// Show / podcast name; rendered as secondary text on the lock
        /// screen and in the Dynamic Island expanded layout.
        public var podcastTitle: String
        /// Current playhead in seconds. Maps to the kernel snapshot's
        /// `PlayerState.positionSecs`.
        public var positionSecs: Double
        /// Episode duration in seconds, or `0` when unknown — the widget
        /// guards against divide-by-zero before rendering progress.
        public var durationSecs: Double
        /// `true` while the engine is actively playing audio; `false`
        /// while paused / buffering / seeking.
        public var isPlaying: Bool
        /// Optional artwork URL. The widget extension fetches via
        /// `AsyncImage`; a nil/invalid URL renders the brand placeholder.
        public var artworkURL: URL?

        public init(
            episodeTitle: String,
            podcastTitle: String,
            positionSecs: Double,
            durationSecs: Double,
            isPlaying: Bool,
            artworkURL: URL?
        ) {
            self.episodeTitle = episodeTitle
            self.podcastTitle = podcastTitle
            self.positionSecs = positionSecs
            self.durationSecs = durationSecs
            self.isPlaying = isPlaying
            self.artworkURL = artworkURL
        }

        /// `0.0..=1.0` fraction of the episode that has been consumed.
        /// Returns `0` for unknown / zero-duration episodes — the widget
        /// renders the ring/bar at empty without dividing by zero.
        public var positionFraction: Double {
            guard durationSecs > 0 else { return 0 }
            return min(max(positionSecs / durationSecs, 0), 1)
        }
    }

    /// Static episode identifier captured when the activity is started.
    /// Used by the widget to render a stable deep-link `widgetURL`
    /// pointing back at the episode in the app. Optional because the
    /// kernel snapshot's `PlayerState.episodeId` is also optional.
    public var episodeID: String?

    public init(episodeID: String? = nil) {
        self.episodeID = episodeID
    }
}
#endif
