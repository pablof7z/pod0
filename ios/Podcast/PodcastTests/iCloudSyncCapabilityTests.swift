import XCTest
@testable import Podcast

// MARK: - iCloudSyncCapability tests
//
// Pins the outbound (snapshot → KVS) path and the snapshot bridge that
// feeds it. The inbound (KVS → kernel dispatch) path requires a live
// `KernelModel` (and therefore a Rust kernel handle); it is exercised
// indirectly via the `isApplyingRemoteChange` echo-suppression flag.
//
// All tests run on the main actor because the capability is
// `@MainActor`-isolated; XCTest tolerates this.

@MainActor
final class iCloudSyncCapabilityTests: XCTestCase {

    // MARK: - SettingsKVSnapshot

    func testEmptySnapshotIsAllNil() {
        let s = SettingsKVSnapshot.empty
        XCTAssertNil(s.speed)
        XCTAssertNil(s.skipForwardSecs)
        XCTAssertNil(s.skipBackwardSecs)
        XCTAssertNil(s.autoSkipAds)
        XCTAssertNil(s.streamingOnly)
    }

    func testSnapshotWriteSeedsLastWrittenOnlyForPresentFields() {
        var last: [String: AnyHashable] = [:]
        let s = SettingsKVSnapshot(speed: 1.5, skipForwardSecs: nil,
                                   skipBackwardSecs: 10, autoSkipAds: nil,
                                   streamingOnly: true)
        s.write(to: &last)
        XCTAssertEqual(last[iCloudSyncCapability.Key.speed], AnyHashable(1.5))
        XCTAssertEqual(last[iCloudSyncCapability.Key.skipBackwardSecs], AnyHashable(10))
        XCTAssertEqual(last[iCloudSyncCapability.Key.streamingOnly], AnyHashable(true))
        XCTAssertNil(last[iCloudSyncCapability.Key.skipForwardSecs])
        XCTAssertNil(last[iCloudSyncCapability.Key.autoSkipAds])
    }

    // MARK: - PodcastUpdate bridge

    func testBridgeReturnsEmptyUntilSettingsProjectionLands() {
        // The bridge is intentionally an `.empty` no-op until the
        // settings projection lands the preference fields on
        // `SettingsSnapshot`. Sourcing `speed` from `nowPlaying.speed`
        // would push the live player rate (which defaults to `1.0` at
        // every episode load) back to iCloud and clobber the user's
        // persisted preference — see the comment on the extension.
        var update = PodcastUpdate()
        update.nowPlaying = PlayerState(speed: 1.75)
        let s = SettingsKVSnapshot.from(podcastUpdate: update)
        XCTAssertEqual(s, .empty,
                       "bridge must not source playback rate from `nowPlaying.speed`")
    }

    // MARK: - Outbound — applySettingsSnapshot

    func testApplySettingsWritesPresentFieldsToKVS() {
        let kvs = NSUbiquitousKeyValueStore.default
        clearTrackedKeys(kvs)
        let cap = iCloudSyncCapability(kvs: kvs)
        // `start()` requires a KernelModel; bypass it for the outbound
        // unit test by manually flipping the started flag via the
        // public `applySettingsSnapshot` no-op guard.
        forceStart(cap)

        cap.applySettingsSnapshot(SettingsKVSnapshot(
            speed: 1.5, skipForwardSecs: 30, skipBackwardSecs: 15,
            autoSkipAds: true, streamingOnly: false))

        XCTAssertEqual((kvs.object(forKey: iCloudSyncCapability.Key.speed)
            as? NSNumber)?.doubleValue, 1.5)
        XCTAssertEqual((kvs.object(forKey: iCloudSyncCapability.Key.skipForwardSecs)
            as? NSNumber)?.intValue, 30)
        XCTAssertEqual((kvs.object(forKey: iCloudSyncCapability.Key.skipBackwardSecs)
            as? NSNumber)?.intValue, 15)
        XCTAssertEqual((kvs.object(forKey: iCloudSyncCapability.Key.autoSkipAds)
            as? NSNumber)?.boolValue, true)
        XCTAssertEqual((kvs.object(forKey: iCloudSyncCapability.Key.streamingOnly)
            as? NSNumber)?.boolValue, false)

        cap.stop()
        clearTrackedKeys(kvs)
    }

    func testApplySettingsSkipsUnchangedFields() {
        // Use a fresh in-memory-ish KVS via the default store; we
        // already pin the lastWritten cache so a second write with the
        // same values is a no-op (verified by reading kvs once and
        // wiping it between the two `apply` calls — second apply must
        // not re-populate).
        let kvs = NSUbiquitousKeyValueStore.default
        clearTrackedKeys(kvs)
        let cap = iCloudSyncCapability(kvs: kvs)
        forceStart(cap)

        let snap = SettingsKVSnapshot(speed: 1.0)
        cap.applySettingsSnapshot(snap)
        XCTAssertNotNil(kvs.object(forKey: iCloudSyncCapability.Key.speed))

        kvs.removeObject(forKey: iCloudSyncCapability.Key.speed)
        cap.applySettingsSnapshot(snap) // same value → no write
        XCTAssertNil(kvs.object(forKey: iCloudSyncCapability.Key.speed),
                     "lastWritten cache should suppress the duplicate write")

        cap.stop()
        clearTrackedKeys(kvs)
    }

    // MARK: - Echo suppression

    func testApplySettingsSwallowsOneTickWhileRemoteChangePending() {
        let kvs = NSUbiquitousKeyValueStore.default
        clearTrackedKeys(kvs)
        let cap = iCloudSyncCapability(kvs: kvs)
        forceStart(cap)

        // Simulate inbound-applying state. The next outbound tick must
        // be swallowed (so the kernel's echo doesn't re-write iCloud)
        // and the lastWritten cache must be seeded from the snapshot
        // the caller provides.
        setApplyingRemote(cap, true)
        let snap = SettingsKVSnapshot(speed: 2.0)
        cap.applySettingsSnapshot(snap)

        XCTAssertNil(kvs.object(forKey: iCloudSyncCapability.Key.speed),
                     "outbound write must be skipped during echo suppression")
        XCTAssertFalse(cap.isApplyingRemoteChange,
                       "flag must clear after the single suppressed tick")
        XCTAssertEqual(cap.lastWritten[iCloudSyncCapability.Key.speed], AnyHashable(2.0),
                       "lastWritten must be seeded from the suppressed snapshot")

        cap.stop()
        clearTrackedKeys(kvs)
    }

    // MARK: - Helpers

    /// Force the capability into the started state without a
    /// `KernelModel`. The outbound path checks `started` but does not
    /// touch `kernel`, so this is safe for these tests.
    private func forceStart(_ cap: iCloudSyncCapability) {
        // `start(kernel:)` is the public entry point but it requires a
        // live model. We flip the gate via a tiny round-trip: passing
        // a value to the inbound `handleExternalChange(changedKeys:)`
        // is a no-op when nothing is tracked, but if we set a non-nil
        // change list with an unknown key the early return preserves
        // `started=false`. So instead we use the dedicated test seam
        // below.
        cap.testForceStarted()
    }

    private func setApplyingRemote(_ cap: iCloudSyncCapability, _ value: Bool) {
        cap.testSetApplyingRemoteChange(value)
    }

    /// Remove every key this capability owns from `kvs` so test cases
    /// see a clean store regardless of order or what the real device
    /// has roaming. We do **not** call `kvs.synchronize()` because it
    /// would block on the entitled iCloud transport — these tests
    /// exercise the in-memory side of the store only.
    private func clearTrackedKeys(_ kvs: NSUbiquitousKeyValueStore) {
        for key in iCloudSyncCapability.Key.all {
            kvs.removeObject(forKey: key)
        }
    }
}
