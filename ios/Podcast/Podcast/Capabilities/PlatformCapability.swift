import Foundation
import os.log

// MARK: - PlatformCapability
//
// iOS half of the kernel-side platform-integration capability
// (namespace `nmp.platform.capability`). The Rust contract is
// declared in `apps/nmp-app-podcast/src/ffi/snapshot.rs`
// (`WidgetSnapshot` field on `PodcastUpdate`),
// `apps/podcast-core/src/types/handoff.rs` (`HandoffState`), and
// `apps/nmp-app-podcast/src/ffi/actions.rs`
// (`ACTION_SIRI_PLAY_LATEST`, `ACTION_SIRI_RESUME`).
//
// This is the **M11 stub**: enough to (a) own the App Group write
// path the widget extension reads from, and (b) expose the
// translation surface for Handoff so subsequent M11 units don't
// need to re-define the contract. The full widget extension,
// AppIntents performers, and Live-Activity executor land in their
// own milestones (M11.C / M11.D).
//
// Doctrine:
//   D6 — failures never throw; an unreachable App Group, an
//        un-encodable snapshot, or a malformed JSON envelope all
//        return without producing user-visible side effects.
//   D7 — this capability reports + executes; it never decides
//        policy. The kernel picks *what* the widget shows by
//        populating `WidgetSnapshot`; iOS only serializes the
//        kernel's choice and hands it to the OS. The kernel picks
//        *whether* to donate a Handoff activity by emitting a
//        `HandoffState`; iOS only translates it.
//
// PASSIVE SHAPE — unlike `AudioCapability` / `KeychainCapability`,
// there is no request/response capability socket here. iOS
// platform extensions (widgets, NSUserActivity, AppIntents) are
// driven by *tick callbacks*, not by kernel-issued requests. So
// `PlatformCapability` is not wired into
// `PodcastCapabilities.handleJSON(_:)`; it exposes `start()/stop()`
// for lifecycle symmetry and a pair of serialization entry points
// the kernel-tick observer invokes on every snapshot advance.

@MainActor
final class PlatformCapability {

    /// Capability namespace. Reserved for future request/response
    /// expansions (e.g. a Siri-donation acknowledgement back to
    /// the kernel). Today this capability is passive — the
    /// namespace is documented so the kernel-side action modules
    /// can target it without re-allocating a string.
    static let namespace = "nmp.platform.capability"

    /// App Group identifier the widget extension reads its
    /// snapshot from. Must match the entitlements on both targets
    /// and the `NowPlayingTimelineProvider.appGroupID` constant
    /// the widget defines locally.
    ///
    /// This is `group.com.podcastr.app` (matches the legacy
    /// `NowPlayingSnapshotStore`) — *not* the app's bundle id.
    /// Per project memory: the bundle id is `io.f7z.podcast`; the
    /// App Group keeps the legacy `com.podcastr.app` namespace so
    /// the existing widget binary continues to find the suite.
    static let appGroupID = "group.com.podcastr.app"

    /// `UserDefaults` key under which the widget snapshot is
    /// stored. Suffixed `.v2` to distinguish from the legacy
    /// `NowPlayingSnapshotStore` key (`now-playing-snapshot.v1`)
    /// the existing widget binary reads — the new key is for the
    /// NMP-derived `WidgetSnapshot` shape, written by this
    /// capability once the widget extension migrates to it.
    static let widgetSnapshotKey = "nmp.widget.snapshot.v1"

    private static let logger = Logger.app("PlatformCapability")

    /// Shared encoder. The kernel emits widget snapshots on every
    /// tick the relevant fields change; allocating a fresh
    /// `JSONEncoder` per write is wasted Foundation churn.
    private let encoder: JSONEncoder = {
        let e = JSONEncoder()
        // The widget extension decodes with snake_case keys (the
        // Rust schema is `now_playing_episode_title`,
        // `is_playing`, …). Match it so the wire round-trips.
        e.keyEncodingStrategy = .convertToSnakeCase
        return e
    }()

    private var started: Bool = false

    /// Most recent activity donated to the OS. Tracked so we can
    /// invalidate on activity-type changes (Apple's API: a single
    /// `NSUserActivity` per scene, swap by `invalidate()` then
    /// `becomeCurrent()`).
    private var currentActivity: NSUserActivity?

    /// Idempotent. Marks the capability active. Today this is a
    /// no-op besides flipping the flag — the OS resources
    /// (`NSUserActivity`, App Group `UserDefaults`) are lazily
    /// touched on first `widgetSnapshot(...)` / `updateHandoff(...)`
    /// call.
    func start() {
        guard !started else { return }
        started = true
    }

    /// Idempotent. Marks the capability inactive. Invalidates any
    /// donated `NSUserActivity` so a subsequent foreground doesn't
    /// continue pointing at stale state.
    func stop() {
        started = false
        currentActivity?.invalidate()
        currentActivity = nil
    }

    var isStarted: Bool { started }

    // MARK: - Widget snapshot serialization

    /// Serialize the kernel's widget projection into the JSON
    /// payload the widget extension decodes.
    ///
    /// The widget extension owns reads against
    /// `UserDefaults(suiteName: appGroupID)`; this method handles
    /// writes. Callers (the kernel-tick observer) pass the
    /// `WidgetSnapshot` slice they pulled off the latest
    /// `PodcastUpdate`. Returns the raw JSON bytes that were
    /// written — also returned so callers that don't have an App
    /// Group entitlement available (unit tests) can still verify
    /// the wire shape.
    ///
    /// D6: a `JSONEncoder` failure or an unreachable App Group
    /// suite logs and degrades to "no write" — the widget falls
    /// back to its empty state on the next timeline refresh.
    @discardableResult
    func writeWidgetSnapshot(_ snapshot: WidgetSnapshot) -> Data? {
        let data: Data
        do {
            data = try encoder.encode(snapshot)
        } catch {
            Self.logger.error("widget snapshot encode failed: \(error, privacy: .public)")
            return nil
        }
        if let defaults = UserDefaults(suiteName: Self.appGroupID) {
            defaults.set(data, forKey: Self.widgetSnapshotKey)
        } else {
            Self.logger.error(
                "app group \(Self.appGroupID, privacy: .public) unreachable; "
                + "widget snapshot dropped")
        }
        return data
    }

    /// Clear the widget snapshot. Called when the kernel emits a
    /// `widget` projection of `nil` (nothing is playing and there
    /// is nothing to surface). The widget extension's next
    /// timeline refresh reads the absent key and shows its empty
    /// state.
    func clearWidgetSnapshot() {
        guard let defaults = UserDefaults(suiteName: Self.appGroupID) else { return }
        defaults.removeObject(forKey: Self.widgetSnapshotKey)
    }

    // MARK: - Handoff (NSUserActivity) translation

    /// Donate a `NSUserActivity` for the kernel's handoff state.
    ///
    /// The kernel decides *whether* to surface Handoff (it emits
    /// `HandoffState` on the snapshot when appropriate); this
    /// method only translates the decision into the OS API. An
    /// unknown `activity_type` (e.g. a future activity id the
    /// kernel started emitting that this binary doesn't know
    /// about) is dropped — D6.
    func donateHandoff(_ state: HandoffState) {
        guard state.isKnownActivityType else {
            Self.logger.debug(
                "dropping handoff with unknown activity type "
                + "\(state.activityType, privacy: .public)")
            return
        }
        currentActivity?.invalidate()
        let activity = NSUserActivity(activityType: state.activityType)
        activity.isEligibleForHandoff = true
        var userInfo: [String: Any] = [:]
        if let id = state.episodeID {
            userInfo[HandoffUserInfoKey.episodeID] = id
        }
        if let id = state.podcastID {
            userInfo[HandoffUserInfoKey.podcastID] = id
        }
        if let pos = state.positionSecs {
            userInfo[HandoffUserInfoKey.positionSecs] = pos
        }
        if !userInfo.isEmpty {
            activity.userInfo = userInfo
        }
        activity.becomeCurrent()
        currentActivity = activity
    }

    /// Invalidate the currently-donated activity. Called when the
    /// kernel emits a `handoff` projection of `nil`.
    func clearHandoff() {
        currentActivity?.invalidate()
        currentActivity = nil
    }
}

// MARK: - Wire types (Swift mirror of the Rust schema)

/// Swift mirror of `apps/nmp-app-podcast/src/ffi/snapshot.rs`
/// `WidgetSnapshot`. Hand-mirrored (not generated) until the
/// `dump_projection_schemas` codegen catches up — keep the field
/// names + `Codable` semantics in lock-step with the Rust source.
///
/// The widget extension decodes the JSON payload this struct
/// encodes; the extension defines its own copy of the type because
/// the two targets don't share Swift sources.
struct WidgetSnapshot: Codable, Equatable {
    var nowPlayingEpisodeTitle: String?
    var nowPlayingPodcastTitle: String?
    var nowPlayingArtworkURL: String?
    var isPlaying: Bool
    /// `0.0..=1.0`; pre-computed by Rust so the widget can render
    /// a ring without dividing by a possibly-zero duration.
    var positionFraction: Float
    var unplayedCount: Int

    enum CodingKeys: String, CodingKey {
        case nowPlayingEpisodeTitle = "now_playing_episode_title"
        case nowPlayingPodcastTitle = "now_playing_podcast_title"
        case nowPlayingArtworkURL = "now_playing_artwork_url"
        case isPlaying = "is_playing"
        case positionFraction = "position_fraction"
        case unplayedCount = "unplayed_count"
    }
}

/// Swift mirror of `apps/podcast-core/src/types/handoff.rs`
/// `HandoffState`. The Rust side guarantees `activityType` is
/// one of the known string ids; `isKnownActivityType` performs
/// the defensive check on the iOS side (D6 — unknown wire data
/// is dropped, not thrown).
struct HandoffState: Codable, Equatable {
    /// `io.f7z.podcast.playing` — playback in progress.
    static let activityPlaying = "io.f7z.podcast.playing"
    /// `io.f7z.podcast.browsing` — non-player surface foregrounded.
    static let activityBrowsing = "io.f7z.podcast.browsing"

    var activityType: String
    var episodeID: String?
    var podcastID: String?
    var positionSecs: Double?

    enum CodingKeys: String, CodingKey {
        case activityType = "activity_type"
        case episodeID = "episode_id"
        case podcastID = "podcast_id"
        case positionSecs = "position_secs"
    }

    /// `true` when `activityType` matches one of the platform
    /// capability's known activity ids.
    var isKnownActivityType: Bool {
        switch activityType {
        case Self.activityPlaying, Self.activityBrowsing:
            return true
        default:
            return false
        }
    }
}

/// `userInfo` keys the iOS executor populates on a donated
/// `NSUserActivity`. The receiving side (the same app on another
/// device) reads back via these keys.
enum HandoffUserInfoKey {
    static let episodeID = "episode_id"
    static let podcastID = "podcast_id"
    static let positionSecs = "position_secs"
}
