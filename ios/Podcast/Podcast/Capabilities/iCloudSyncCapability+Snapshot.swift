import Foundation

// MARK: - SettingsKVSnapshot
//
// Decoupled snapshot of the kernel-side fields the iCloud sync capability
// mirrors. Defined here (not on `SettingsSnapshot`) so the capability does
// not need to wait for `pr-settings-projection` to land the playback fields
// on the projection. The snapshot poll constructs one of these from
// whatever subset of fields the current `SettingsSnapshot` exposes; missing
// fields stay `nil` and are simply skipped on the outbound path.

/// Plain-data view of the subset of settings the iCloud capability mirrors.
/// Fields are optional because the `SettingsSnapshot` projection may not yet
/// carry every field on every binary — a `nil` field is a "kernel hasn't
/// projected this yet, skip" signal.
struct SettingsKVSnapshot: Equatable {
    var speed: Double?
    var skipForwardSecs: Int?
    var skipBackwardSecs: Int?
    var autoSkipAds: Bool?
    var streamingOnly: Bool?

    /// All-`nil` snapshot. Returned by the `PodcastUpdate` bridge when the
    /// active kernel projection does not (yet) carry any of the playback
    /// fields the capability syncs.
    static let empty = SettingsKVSnapshot()

    /// Seed the capability's `lastWritten` map with this snapshot's present
    /// fields. Used during inbound merges so the next outbound diff does
    /// not re-emit the value we just applied.
    func write(to lastWritten: inout [String: AnyHashable]) {
        if let speed {
            lastWritten[iCloudSyncCapability.Key.speed] = AnyHashable(speed)
        }
        if let v = skipForwardSecs {
            lastWritten[iCloudSyncCapability.Key.skipForwardSecs] = AnyHashable(v)
        }
        if let v = skipBackwardSecs {
            lastWritten[iCloudSyncCapability.Key.skipBackwardSecs] = AnyHashable(v)
        }
        if let v = autoSkipAds {
            lastWritten[iCloudSyncCapability.Key.autoSkipAds] = AnyHashable(v)
        }
        if let v = streamingOnly {
            lastWritten[iCloudSyncCapability.Key.streamingOnly] = AnyHashable(v)
        }
    }
}

// MARK: - PodcastUpdate bridge
//
// Maps the current kernel `PodcastUpdate` into the subset of fields this
// capability mirrors to iCloud. Every field is sourced from the
// kernel-owned **settings preference** (not from transient state like
// `nowPlaying.speed`, which is the *currently playing* rate and defaults
// to `1.0` every time a new episode loads — pushing it back to iCloud
// would clobber the user's persisted preference).
//
// The settings-projection work lands the four scalar fields on
// `SettingsSnapshot` in `pr-settings-projection`. Until those fields
// exist on the generated type this extension returns `.empty` so the
// outbound path is a guaranteed no-op. When the projection lands, this
// extension is the **only** site that needs to change — the capability
// itself is already wired.

extension SettingsKVSnapshot {
    /// Build a snapshot from the current kernel `PodcastUpdate`. Returns
    /// `.empty` when the active `PodcastUpdate` does not yet carry the
    /// playback-rate / skip-interval / auto-skip-ads / streaming-only
    /// fields on `settings` (the pre-`pr-settings-projection` shape).
    ///
    /// When the projection lands, replace the body with explicit reads
    /// from the **preference** fields (not from `nowPlaying.speed` —
    /// see the explanation above):
    ///
    /// ```swift
    /// SettingsKVSnapshot(
    ///     speed: update.settings.playbackSpeed,
    ///     skipForwardSecs: update.settings.skipForwardSecs,
    ///     skipBackwardSecs: update.settings.skipBackwardSecs,
    ///     autoSkipAds: update.settings.autoSkipAds,
    ///     streamingOnly: update.settings.streamingOnly)
    /// ```
    static func from(podcastUpdate _: PodcastUpdate) -> SettingsKVSnapshot {
        .empty
    }
}
