import Foundation
import os.log

// MARK: - iCloudSyncCapability — `pcst.icloud_sync.capability`
//
// iOS half of the iCloud-settings-sync surface (feature #52). Mirrors a
// curated subset of `podcast.settings` into `NSUbiquitousKeyValueStore` so
// playback speed, skip intervals, ad-skip toggle, and streaming-only toggle
// roam across the user's devices and survive a reinstall.
//
// PASSIVE SHAPE — like `PlatformCapability` / `SpotlightCapability` there
// is no request/response capability socket here. The capability is driven
// by (a) `NSUbiquitousKeyValueStoreDidChangeExternallyNotification` for
// inbound pulls and (b) the snapshot-poll observer in
// `KernelModel.startSnapshotPoll` for outbound writes. It is therefore
// **not** routed through `PodcastCapabilities.handleJSON(_:)`.
//
// Doctrine:
//   D6 — failures never throw. A missing entitlement, an unreachable iCloud
//        account, or a kernel-action rejection all degrade silently. The
//        inbound path uses `KernelModel.dispatchSilent` so a not-yet-wired
//        `podcast.settings.*` action does not surface as a user-visible
//        toast.
//   D7 — iOS executes; Rust decides. The cloud value is dispatched as an
//        action; the kernel may clamp, reject, or transform it before
//        emitting the next snapshot. We then read the next snapshot and
//        only write back if the kernel's authoritative value differs from
//        cloud (the `lastWritten` diff cache in `applySettingsSnapshot`).

/// `NSUbiquitousKeyValueStore` mirror for the podcast app's portable
/// settings. Single instance, owned by `PodcastCapabilities`. A weak
/// reference to `KernelModel` is the dispatch surface for inbound changes;
/// the model holds this capability strongly so the weak handle stays valid
/// for the lifetime of the app.
@MainActor
final class iCloudSyncCapability {

    // MARK: - Constants

    /// Capability namespace. Reserved for future request/response wiring;
    /// today this capability is purely tick-driven.
    static let namespace = "pcst.icloud_sync.capability"

    /// Key namespace for `NSUbiquitousKeyValueStore`. Kept short — the
    /// store has a 1024-byte per-key budget and the four scalar values fit
    /// comfortably with these names.
    enum Key {
        static let speed             = "pcst.speed"
        static let skipForwardSecs   = "pcst.skip_forward_secs"
        static let skipBackwardSecs  = "pcst.skip_backward_secs"
        static let autoSkipAds       = "pcst.auto_skip_ads"
        static let streamingOnly     = "pcst.streaming_only"

        /// Every key this capability owns. Used by the external-change
        /// observer to filter `NSUbiquitousKeyValueStoreChangedKeysKey`
        /// down to just our slots.
        static let all: Set<String> = [
            speed, skipForwardSecs, skipBackwardSecs, autoSkipAds, streamingOnly,
        ]
    }

    // MARK: - State

    private static let logger = Logger(subsystem: "io.f7z.podcast", category: "iCloudSync")

    private let kvs: NSUbiquitousKeyValueStore

    /// Dispatch sink. Weak so the capability does not extend the model's
    /// lifetime — the model owns the capability, not the other way round.
    weak var kernel: KernelModel?

    /// Retained observer token. Cleared in `stop()` so the notification
    /// centre does not call back into a stopped capability.
    private var changeObserver: NSObjectProtocol?

    /// Snapshot of the last value we wrote to (or read from) iCloud for
    /// each key. Used by `applySettingsSnapshot(_:)` to detect deltas so
    /// we only write when the value actually changed.
    private(set) var lastWritten: [String: AnyHashable] = [:]

    /// Echo-suppression flag. Set while applying an inbound iCloud change
    /// so the outbound writer does not immediately re-emit the same value.
    private(set) var isApplyingRemoteChange: Bool = false

    private var started: Bool = false

    // MARK: - Init

    init(kvs: NSUbiquitousKeyValueStore = .default) {
        self.kvs = kvs
    }

    // MARK: - Lifecycle

    /// Register for change notifications, kick off an initial KVS pull,
    /// and dispatch any non-default cloud values into the kernel. Safe to
    /// call multiple times — subsequent calls are no-ops.
    func start(kernel: KernelModel) {
        guard !started else { return }
        started = true
        self.kernel = kernel

        // Subscribe before the synchronize() — there is no race window in
        // which an external change could fire before we are listening.
        // Extract the changed-keys list inside the (non-Sendable)
        // notification closure and hop just the `[String]` across the
        // `@MainActor` boundary so Swift 6 doesn't flag the notification
        // capture as a data race.
        changeObserver = NotificationCenter.default.addObserver(
            forName: NSUbiquitousKeyValueStore.didChangeExternallyNotification,
            object: kvs,
            queue: .main
        ) { [weak self] notification in
            let changed = notification.userInfo?[NSUbiquitousKeyValueStoreChangedKeysKey]
                as? [String] ?? []
            MainActor.assumeIsolated {
                self?.handleExternalChange(changedKeys: changed)
            }
        }

        kvs.synchronize()
        dispatchKeysFromCloud(Array(Key.all))
        Self.logger.info("iCloudSyncCapability started")
    }

    /// Idempotent. Tears down the change observer and clears the dispatch
    /// handle. Does **not** clear `NSUbiquitousKeyValueStore` — those
    /// values live in the user's iCloud account.
    func stop() {
        started = false
        if let observer = changeObserver {
            NotificationCenter.default.removeObserver(observer)
        }
        changeObserver = nil
        kernel = nil
        lastWritten = [:]
        isApplyingRemoteChange = false
    }

    var isStarted: Bool { started }

    // MARK: - Outbound — snapshot → iCloud

    /// Compare `settings` against the last value we wrote and push any
    /// changed keys to iCloud. Called by the snapshot-poll observer in
    /// `KernelModel.startSnapshotPoll` on every tick where
    /// `podcastSnapshot` advanced.
    func applySettingsSnapshot(_ settings: SettingsKVSnapshot) {
        guard started else { return }
        if isApplyingRemoteChange {
            // Single-tick suppression. Seed `lastWritten` with the
            // kernel's view so the next genuine local edit *is* written.
            isApplyingRemoteChange = false
            settings.write(to: &lastWritten)
            return
        }
        if let v = settings.speed,
           lastWritten[Key.speed] != AnyHashable(v) {
            kvs.set(v, forKey: Key.speed)
            lastWritten[Key.speed] = AnyHashable(v)
        }
        if let v = settings.skipForwardSecs,
           lastWritten[Key.skipForwardSecs] != AnyHashable(v) {
            kvs.set(Int64(v), forKey: Key.skipForwardSecs)
            lastWritten[Key.skipForwardSecs] = AnyHashable(v)
        }
        if let v = settings.skipBackwardSecs,
           lastWritten[Key.skipBackwardSecs] != AnyHashable(v) {
            kvs.set(Int64(v), forKey: Key.skipBackwardSecs)
            lastWritten[Key.skipBackwardSecs] = AnyHashable(v)
        }
        if let v = settings.autoSkipAds,
           lastWritten[Key.autoSkipAds] != AnyHashable(v) {
            kvs.set(v, forKey: Key.autoSkipAds)
            lastWritten[Key.autoSkipAds] = AnyHashable(v)
        }
        if let v = settings.streamingOnly,
           lastWritten[Key.streamingOnly] != AnyHashable(v) {
            kvs.set(v, forKey: Key.streamingOnly)
            lastWritten[Key.streamingOnly] = AnyHashable(v)
        }
    }

    // MARK: - Inbound — iCloud → snapshot

    /// Handle a single
    /// `NSUbiquitousKeyValueStoreDidChangeExternallyNotification`. Takes
    /// the changed-keys list (extracted on the producing side so the hop
    /// across the `@MainActor` boundary stays Sendable) and dispatches
    /// the matching `podcast.settings.*` action for each tracked key.
    ///
    /// Internal so the test suite can drive this path directly without
    /// having to fire an actual KVS notification.
    func handleExternalChange(changedKeys: [String]) {
        let tracked = changedKeys.filter { Key.all.contains($0) }
        guard !tracked.isEmpty else { return }
        Self.logger.info("KVS external change: \(tracked.joined(separator: ","), privacy: .public)")
        dispatchKeysFromCloud(tracked)
    }

    /// Shared dispatch path used by both the on-launch merge and the
    /// external-change observer. Skip-interval requires both values
    /// together (the action takes `forward` + `backward`) so the two
    /// keys are coalesced into a single dispatch.
    ///
    /// Dispatched via `KernelModel.dispatchSilent` — a rejection from a
    /// not-yet-wired Rust action should not surface as a user toast.
    private func dispatchKeysFromCloud(_ keys: [String]) {
        let touched = Set(keys)
        var didDispatch = false

        if touched.contains(Key.speed),
           let speed = (kvs.object(forKey: Key.speed) as? NSNumber)?.doubleValue,
           lastWritten[Key.speed] != AnyHashable(speed) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_speed", "speed": speed])
            lastWritten[Key.speed] = AnyHashable(speed)
            didDispatch = true
        }

        if touched.contains(Key.skipForwardSecs) || touched.contains(Key.skipBackwardSecs),
           let forward = (kvs.object(forKey: Key.skipForwardSecs) as? NSNumber)?.intValue,
           let backward = (kvs.object(forKey: Key.skipBackwardSecs) as? NSNumber)?.intValue,
           lastWritten[Key.skipForwardSecs] != AnyHashable(forward)
             || lastWritten[Key.skipBackwardSecs] != AnyHashable(backward) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings", body: [
                "op": "set_skip_intervals",
                "forward_secs": Double(forward),
                "backward_secs": Double(backward),
            ])
            lastWritten[Key.skipForwardSecs] = AnyHashable(forward)
            lastWritten[Key.skipBackwardSecs] = AnyHashable(backward)
            didDispatch = true
        }

        if touched.contains(Key.autoSkipAds),
           let enabled = (kvs.object(forKey: Key.autoSkipAds) as? NSNumber)?.boolValue,
           lastWritten[Key.autoSkipAds] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_auto_skip_ads", "enabled": enabled])
            lastWritten[Key.autoSkipAds] = AnyHashable(enabled)
            didDispatch = true
        }

        if touched.contains(Key.streamingOnly),
           let enabled = (kvs.object(forKey: Key.streamingOnly) as? NSNumber)?.boolValue,
           lastWritten[Key.streamingOnly] != AnyHashable(enabled) {
            isApplyingRemoteChange = true
            kernel?.dispatchSilent(namespace: "podcast.settings",
                                   body: ["op": "set_streaming_only", "enabled": enabled])
            lastWritten[Key.streamingOnly] = AnyHashable(enabled)
            didDispatch = true
        }

        // Nothing actually dispatched → no kernel echo to suppress; reset
        // the flag so the next outbound tick is not swallowed.
        if !didDispatch {
            isApplyingRemoteChange = false
        }
    }
}

// `SettingsKVSnapshot` (the value the snapshot-poll bridge produces) and
// the `from(podcastUpdate:)` bridge live in
// `iCloudSyncCapability+Snapshot.swift`.

#if DEBUG
extension iCloudSyncCapability {
    /// Test-only seam: drive the capability into the started state
    /// without a `KernelModel`. The outbound path checks `started` but
    /// not `kernel`, so this enables outbound-only unit tests that do
    /// not need a live Rust kernel handle. Compiled out of release
    /// builds.
    func testForceStarted() {
        started = true
    }

    /// Test-only seam: directly flip the echo-suppression flag so the
    /// outbound-skip behaviour can be exercised without round-tripping
    /// through an external KVS notification.
    func testSetApplyingRemoteChange(_ value: Bool) {
        isApplyingRemoteChange = value
    }
}
#endif
