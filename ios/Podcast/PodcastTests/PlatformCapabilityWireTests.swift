import XCTest
@testable import Podcast

// MARK: - PlatformCapability wire-shape tests
//
// The Rust contract for the platform-integration capability spans two
// files: `apps/nmp-app-podcast/src/ffi/snapshot.rs` (`WidgetSnapshot`
// on `PodcastUpdate`) and `apps/podcast-core/src/types/handoff.rs`
// (`HandoffState`). The Swift mirrors live in
// `Capabilities/PlatformCapability.swift`.
//
// These tests pin the JSON shape on the Swift side so a drift on
// either side fails the build. The Rust crates have the equivalent
// round-trips on their side (`ffi::snapshot::tests::*`,
// `types::handoff::tests::*`); together they form a two-sided
// contract.

final class PlatformCapabilityWireTests: XCTestCase {

    // MARK: - WidgetSnapshot

    func testWidgetSnapshotRoundTripsThroughJSON() throws {
        let original = WidgetSnapshot(
            nowPlayingEpisodeTitle: "Ep 42",
            nowPlayingPodcastTitle: "Some Show",
            nowPlayingArtworkURL: "https://ex.com/art.png",
            isPlaying: true,
            positionFraction: 0.42,
            unplayedCount: 7)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(WidgetSnapshot.self, from: data)
        XCTAssertEqual(decoded, original)
    }

    func testWidgetSnapshotEncodesSnakeCaseKeys() throws {
        let snap = WidgetSnapshot(
            nowPlayingEpisodeTitle: "Ep 1",
            nowPlayingPodcastTitle: nil,
            nowPlayingArtworkURL: nil,
            isPlaying: false,
            positionFraction: 0.0,
            unplayedCount: 0)
        let data = try JSONEncoder().encode(snap)
        let json = String(data: data, encoding: .utf8) ?? ""
        XCTAssertTrue(json.contains("\"now_playing_episode_title\":\"Ep 1\""))
        XCTAssertTrue(json.contains("\"is_playing\":false"))
        XCTAssertTrue(json.contains("\"position_fraction\":0"))
        XCTAssertTrue(json.contains("\"unplayed_count\":0"))
    }

    func testWidgetSnapshotDecodesRustEmittedPayload() throws {
        // This JSON is byte-identical to what the Rust encoder emits
        // for `WidgetSnapshot::default()` plus a populated title;
        // pinned here so a Rust-side rename surfaces on the iOS test
        // suite immediately.
        let json = """
        {"now_playing_episode_title":"Ep 1","is_playing":true,\
        "position_fraction":0.5,"unplayed_count":3}
        """
        let data = Data(json.utf8)
        let decoded = try JSONDecoder().decode(WidgetSnapshot.self, from: data)
        XCTAssertEqual(decoded.nowPlayingEpisodeTitle, "Ep 1")
        XCTAssertTrue(decoded.isPlaying)
        XCTAssertEqual(decoded.positionFraction, 0.5)
        XCTAssertEqual(decoded.unplayedCount, 3)
        XCTAssertNil(decoded.nowPlayingPodcastTitle)
        XCTAssertNil(decoded.nowPlayingArtworkURL)
    }

    // MARK: - HandoffState

    func testHandoffActivityIdsMatchRustConstants() {
        XCTAssertEqual(HandoffState.activityPlaying, "io.f7z.podcast.playing")
        XCTAssertEqual(HandoffState.activityBrowsing, "io.f7z.podcast.browsing")
    }

    func testHandoffStateRoundTripsThroughJSON() throws {
        let original = HandoffState(
            activityType: HandoffState.activityPlaying,
            episodeID: "ep-1",
            podcastID: nil,
            positionSecs: 42.5)
        let data = try JSONEncoder().encode(original)
        let decoded = try JSONDecoder().decode(HandoffState.self, from: data)
        XCTAssertEqual(decoded, original)
        XCTAssertTrue(decoded.isKnownActivityType)
    }

    func testHandoffStateDecodesRustEmittedPlayingPayload() throws {
        // Pinned wire shape — matches `HandoffState::playing("ep-1", 42.5)`
        // on the Rust side. The browsing/podcast_id-only variant is
        // covered by the round-trip test above plus the rust unit
        // tests in `types::handoff::tests`.
        let json = """
        {"activity_type":"io.f7z.podcast.playing","episode_id":"ep-1",\
        "position_secs":42.5}
        """
        let data = Data(json.utf8)
        let decoded = try JSONDecoder().decode(HandoffState.self, from: data)
        XCTAssertEqual(decoded.activityType, HandoffState.activityPlaying)
        XCTAssertEqual(decoded.episodeID, "ep-1")
        XCTAssertEqual(decoded.positionSecs, 42.5)
        XCTAssertNil(decoded.podcastID)
    }

    func testHandoffStateUnknownActivityIsNotKnown() {
        let state = HandoffState(
            activityType: "io.f7z.podcast.future_activity",
            episodeID: nil,
            podcastID: nil,
            positionSecs: nil)
        XCTAssertFalse(state.isKnownActivityType)
    }

    // MARK: - PlatformCapability lifecycle

    @MainActor
    func testPlatformCapabilityStartStopIsIdempotent() {
        let cap = PlatformCapability()
        XCTAssertFalse(cap.isStarted)
        cap.start()
        XCTAssertTrue(cap.isStarted)
        cap.start()  // double-start is a no-op
        XCTAssertTrue(cap.isStarted)
        cap.stop()
        XCTAssertFalse(cap.isStarted)
        cap.stop()  // double-stop is a no-op
        XCTAssertFalse(cap.isStarted)
    }

    @MainActor
    func testWriteWidgetSnapshotReturnsEncodedBytesEvenWithoutAppGroup() throws {
        // The unit-test target has no App Group entitlement, so the
        // `UserDefaults(suiteName:)` write is dropped — but the encode
        // path still runs and returns the bytes so callers can verify
        // the wire shape in tests.
        let cap = PlatformCapability()
        let snap = WidgetSnapshot(
            nowPlayingEpisodeTitle: "Ep 1",
            nowPlayingPodcastTitle: nil,
            nowPlayingArtworkURL: nil,
            isPlaying: true,
            positionFraction: 0.25,
            unplayedCount: 5)
        let data = try XCTUnwrap(cap.writeWidgetSnapshot(snap))
        let decoded = try JSONDecoder().decode(WidgetSnapshot.self, from: data)
        XCTAssertEqual(decoded, snap)
    }

    @MainActor
    func testDonateHandoffWithUnknownActivityIsDropped() {
        // D6 — unknown activity types are silently dropped, not thrown.
        // We can't easily observe `becomeCurrent()` from a unit test,
        // but we can at least verify the capability doesn't crash.
        let cap = PlatformCapability()
        cap.start()
        let state = HandoffState(
            activityType: "io.f7z.podcast.future_activity",
            episodeID: nil,
            podcastID: nil,
            positionSecs: nil)
        cap.donateHandoff(state)
        // Reaching here means no crash; the activity should not have
        // been donated. `clearHandoff()` should also be safe to call.
        cap.clearHandoff()
        cap.stop()
    }
}
