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

    // Dedup keys for `applyNowPlayingSnapshot`. All fields written to
    // `NowPlayingSnapshot` are compared except `positionSecs` (excluded
    // intentionally — position-only ticks are handled by
    // `PlaybackState.writeNowPlayingSnapshot`). Comparing every written field
    // ensures library-hydration passes (showName, episodeTitle, imageURL,
    // duration) always write through instead of being blocked by stale state.
    private var lastNowPlayingEpisodeId: String? = nil
    private var lastNowPlayingIsPlaying: Bool = false
    private var lastNowPlayingChapterTitle: String? = nil
    private var lastNowPlayingEpisodeTitle: String = ""
    private var lastNowPlayingShowName: String = ""
    private var lastNowPlayingImageURLString: String? = nil
    private var lastNowPlayingDurationSecs: Double = 0

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

    // MARK: - Now-playing widget (NowPlayingSnapshot path)

    /// Translate the kernel's player state into a `NowPlayingSnapshot` and
    /// write it to the App Group so the widget picks it up. Called by the
    /// kernel-projection observer on every `onNowPlayingSnapshot` tick.
    ///
    /// Deduplicates on all written fields except `positionSecs` — the most
    /// common ticks during live playback change only position, which is
    /// excluded so those ticks don't waste App Group writes. Position is kept
    /// fresh by `PlaybackState.writeNowPlayingSnapshot` (throttled to 5 s).
    /// All other fields are compared so library-hydration passes always win.
    func applyNowPlayingSnapshot(_ snapshot: PodcastUpdate?, library: [PodcastSummary]) {
        guard let nowPlaying = snapshot?.nowPlaying,
              let episodeIdStr = nowPlaying.episodeId else { return }
        let isPlaying = nowPlaying.isPlaying
        let chapterTitle = nowPlaying.currentChapterTitle
        var episodeTitle = episodeIdStr
        var showName = ""
        var imageURLString: String? = nil
        outer: for pod in library {
            for ep in pod.episodes where ep.id == episodeIdStr {
                episodeTitle = ep.title
                showName = pod.title
                imageURLString = ep.artworkUrl ?? pod.artworkUrl
                break outer
            }
        }
        let durationSecs = nowPlaying.durationSecs
        if episodeIdStr == lastNowPlayingEpisodeId,
           isPlaying == lastNowPlayingIsPlaying,
           chapterTitle == lastNowPlayingChapterTitle,
           episodeTitle == lastNowPlayingEpisodeTitle,
           showName == lastNowPlayingShowName,
           imageURLString == lastNowPlayingImageURLString,
           durationSecs == lastNowPlayingDurationSecs { return }
        let episodeChanged = (episodeIdStr != lastNowPlayingEpisodeId)
        lastNowPlayingEpisodeId = episodeIdStr
        lastNowPlayingIsPlaying = isPlaying
        lastNowPlayingChapterTitle = chapterTitle
        lastNowPlayingEpisodeTitle = episodeTitle
        lastNowPlayingShowName = showName
        lastNowPlayingImageURLString = imageURLString
        lastNowPlayingDurationSecs = durationSecs
        // Preserve the live playhead only on same-episode metadata refreshes.
        // The kernel snapshot excludes position-only ticks from its content hash,
        // so nowPlaying.positionSecs can be far behind the real playhead. On an
        // episode change (Siri / kernel auto-advance), use the kernel position
        // so a new episode doesn't inherit the old 40-minute playhead.
        let livePosition: TimeInterval
        if !episodeChanged, let cached = NowPlayingSnapshotStore.lastWrittenSnapshot?.position {
            livePosition = cached
        } else {
            livePosition = nowPlaying.positionSecs
        }
        NowPlayingSnapshotStore.write(NowPlayingSnapshot(
            episodeTitle: episodeTitle,
            showName: showName,
            imageURLString: imageURLString,
            position: livePosition,
            duration: nowPlaying.durationSecs,
            chapterTitle: chapterTitle,
            isPlaying: isPlaying
        ))
    }

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
                "app group \(Self.appGroupID, privacy: .public) unreachable; widget snapshot dropped")
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
    ///
    /// `title` populates `NSUserActivity.title`, which Handoff
    /// renders on the receiving device's lock-screen banner and
    /// the Mac Dock badge. The kernel `HandoffState` carries
    /// transport identifiers only (episode/podcast/position) —
    /// display strings are looked up from the library at donation
    /// time by the iOS caller and passed in here so the wire
    /// shape stays narrow.
    func donateHandoff(_ state: HandoffState, title: String? = nil) {
        guard state.isKnownActivityType else {
            Self.logger.debug(
                "dropping handoff with unknown activity type \(state.activityType, privacy: .public)")
            return
        }
        currentActivity?.invalidate()
        let activity = NSUserActivity(activityType: state.activityType)
        activity.isEligibleForHandoff = true
        if let title, !title.isEmpty {
            activity.title = title
        }
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
        activity.needsSave = true
        activity.becomeCurrent()
        currentActivity = activity
    }

    /// Convenience wrapper for the playback activity. Builds a
    /// `HandoffState(activityType: activityPlaying, ...)` and
    /// delegates to `donateHandoff` so the activity-type /
    /// userInfo encoding stays in one place.
    ///
    /// Used by the snapshot observer in `PodcastApp` while the
    /// kernel projection is still iOS-driven (M11 stub). Once
    /// the kernel begins emitting `HandoffState` directly, the
    /// observer can switch to calling `donateHandoff` with the
    /// emitted state.
    func donatePlayback(
        episodeID: String,
        podcastID: String? = nil,
        episodeTitle: String? = nil,
        positionSecs: Double? = nil
    ) {
        let state = HandoffState(
            activityType: HandoffState.activityPlaying,
            episodeID: episodeID,
            podcastID: podcastID,
            positionSecs: positionSecs)
        donateHandoff(state, title: episodeTitle)
    }

    /// Invalidate the currently-donated activity. Called when the
    /// kernel emits a `handoff` projection of `nil`.
    func clearHandoff() {
        currentActivity?.invalidate()
        currentActivity = nil
    }
}

// Wire types (`WidgetSnapshot`, `HandoffState`, `HandoffUserInfoKey`)
// live in `PlatformCapability+WireTypes.swift` so this capability file
// stays under the soft 300-line limit per AGENTS.md.
