import Foundation
import os.log
import WidgetKit

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
    /// This is `group.com.podcastr.app` — *not* the app's bundle id.
    /// Per project memory: the bundle id is `io.f7z.podcast`; the
    /// App Group keeps the `com.podcastr.app` namespace so the
    /// widget binary continues to find the suite.
    static let appGroupID = "group.com.podcastr.app"

    /// `UserDefaults` key under which the kernel-owned `WidgetSnapshot`
    /// is stored. The widget extension reads exactly this key
    /// (`NowPlayingTimelineProvider.defaultsKey`); it is the single
    /// canonical widget channel (D4) — the old Swift-derived
    /// `now-playing-snapshot.v1` path was deleted.
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

    // Throttle counter for `applyPositionTick`. Position-only App Group writes
    // are capped to ~1 per 5 s (at 1 Hz) — WidgetKit timeline reloads are
    // expensive and position granularity beyond 5 s is imperceptible on the
    // widget face.
    private var positionTickCount = 0

    // Last `WidgetSnapshot` written to the App Group. The canonical change-gate:
    // `applyWidgetSnapshot` only writes (+ reloads timelines) when the kernel's
    // snapshot differs from this, with `positionFraction` compared at 1%
    // quantization (see `fractionStep`) so a continuously-drifting playhead
    // doesn't burn a WidgetKit reload on every tick. Nil means "nothing
    // written yet / cleared".
    private var lastWrittenWidget: WidgetSnapshot? = nil

    /// Quantization step for `positionFraction` comparison: the widget face
    /// renders a ~150 pt progress bar, so sub-1% changes are imperceptible.
    /// Position-only ticks therefore write at most ~100×/episode (once per 1%
    /// crossed) instead of once per second.
    private static let fractionStep: Float = 0.01

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

    // MARK: - Now-playing widget (kernel WidgetSnapshot path)

    /// Apply the kernel's `WidgetSnapshot` to the App Group, the single
    /// canonical widget write path (D4). Wired to `AppStateStore`'s
    /// `onNowPlayingSnapshot` — a content-changed projection tick (episode /
    /// play-pause / chapter / library / unplayed change). The kernel owns the
    /// shape and the derivation; iOS only serializes the kernel's choice.
    ///
    /// Change-gated: writes (and triggers a WidgetKit timeline reload) only
    /// when the snapshot differs from the last write, with `positionFraction`
    /// compared at 1% quantization (`fractionStep`) so a drifting playhead on
    /// a metadata-stable tick doesn't burn a reload. A `nil` widget (nothing
    /// playing and nothing unplayed to badge) clears the App Group key so the
    /// widget renders its empty state.
    ///
    /// Returns `true` when the App Group state changed (a write or a clear
    /// happened), `false` on a deduplicated no-op — exposed so tests can pin
    /// the cadence without inspecting `UserDefaults`.
    @discardableResult
    func applyWidgetSnapshot(_ snapshot: PodcastUpdate?) -> Bool {
        guard let widget = snapshot?.widget else {
            // Kernel says there's nothing to surface — clear once, then no-op
            // on subsequent nil ticks.
            guard lastWrittenWidget != nil else { return false }
            lastWrittenWidget = nil
            clearWidgetSnapshot()
            WidgetCenter.shared.reloadAllTimelines()
            return true
        }
        guard widgetChanged(widget, from: lastWrittenWidget) else { return false }
        lastWrittenWidget = widget
        writeWidgetSnapshot(widget)
        WidgetCenter.shared.reloadAllTimelines()
        // Reset the position-tick throttle so the next tick after a full write
        // produces a fresh position update promptly.
        positionTickCount = 0
        return true
    }

    /// Throttled position-only update. Wired to `AppStateStore.onPositionTick`
    /// (1 Hz kernel heartbeat) — fires on every 5th tick (~5 s) to keep the
    /// widget's progress ring fresh between content-changed snapshots (those
    /// suppress position-only ticks via the kernel's snapshot content hash, so
    /// `WidgetSnapshot.positionFraction` would otherwise go stale during a long
    /// uninterrupted listen). Recomputes the fraction on the last-written
    /// widget and writes only when the 1%-quantized fraction actually moved.
    func applyPositionTick(_ position: Double) {
        positionTickCount += 1
        guard positionTickCount >= 5 else { return }
        positionTickCount = 0
        guard var widget = lastWrittenWidget, widget.durationSecs > 0 else { return }
        let fraction = Float(min(max(position / widget.durationSecs, 0), 1))
        guard quantizedFraction(fraction) != quantizedFraction(widget.positionFraction) else {
            // Still update the cached raw position so the next content tick
            // diff sees the latest playhead, but skip the App Group write +
            // reload — the ring wouldn't visibly move.
            widget.positionSecs = position
            lastWrittenWidget = widget
            return
        }
        widget.positionSecs = position
        widget.positionFraction = fraction
        lastWrittenWidget = widget
        writeWidgetSnapshot(widget)
        WidgetCenter.shared.reloadAllTimelines()
    }

    /// Whether `next` differs from `previous` in any field that affects the
    /// rendered widget, treating `positionFraction` at 1% granularity.
    private func widgetChanged(_ next: WidgetSnapshot, from previous: WidgetSnapshot?) -> Bool {
        guard let previous else { return true }
        if next.nowPlayingEpisodeTitle != previous.nowPlayingEpisodeTitle { return true }
        if next.nowPlayingPodcastTitle != previous.nowPlayingPodcastTitle { return true }
        if next.nowPlayingArtworkURL != previous.nowPlayingArtworkURL { return true }
        if next.nowPlayingChapterTitle != previous.nowPlayingChapterTitle { return true }
        if next.isPlaying != previous.isPlaying { return true }
        if next.durationSecs != previous.durationSecs { return true }
        if next.unplayedCount != previous.unplayedCount { return true }
        if quantizedFraction(next.positionFraction) != quantizedFraction(previous.positionFraction) {
            return true
        }
        return false
    }

    /// Snap a fraction to its 1% bucket for change comparison.
    private func quantizedFraction(_ fraction: Float) -> Int {
        Int((fraction / Self.fractionStep).rounded())
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
