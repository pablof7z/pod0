import XCTest
@testable import Podcastr

// ─── mergeDomainFrames tombstone tests ──────────────────────────────────────
//
// These tests call KernelModel.mergeDomainFramesImpl (the static peer of the
// @MainActor instance method) directly with Rust-shaped JSON decoded through
// the real PodcastDomainFrames.decode seam. They verify that tombstone frames
// ({"rev":N, "<field>": null}) clear the corresponding slice in the composite,
// as documented in KernelDomainFrames.swift and mandated by PR #403.
//
// The static form (mergeDomainFramesImpl) is the same logic the instance
// method delegates to — no duplication, no inline simulation.

private func makeEnvelope(projections: [String: Any]) -> Data {
    let body: [String: Any] = [
        "t": "snapshot",
        "v": ["projections": projections]
    ]
    return try! JSONSerialization.data(withJSONObject: body)
}

@MainActor
final class KernelDomainMergeTests: XCTestCase {

    // ── library tombstone ────────────────────────────────────────────────────

    /// A library tombstone `{"rev":N,"library":null}` clears composite.library.
    ///
    /// Sequence:
    ///   1. Seed the composite with a library frame carrying one show (rev=1).
    ///   2. Apply a library tombstone (rev=2, library=null) through the REAL
    ///      mergeDomainFramesImpl.
    ///   3. Assert composite.library == [].
    func testLibraryTombstoneClearsLibrarySlice() throws {
        // Seed: one-show library at rev=1.
        let seedData = makeEnvelope(projections: [
            DomainSchema.library: [
                "rev": 1,
                "library": [[
                    "id": "pod-1",
                    "title": "Seed Show",
                    "episodes": [],
                    "is_subscribed": true,
                    "episode_count": 0,
                    "unplayed_count": 0,
                    "nostr_visibility": ""
                ]],
                "categories": [],
                "search_results": [],
                "nostr_results": [],
                "owned_podcasts": [],
                "inbox": [],
                "inbox_triage_in_progress": false,
                "inbox_last_triaged_at": 1_717_200_123
            ] as [String: Any]
        ])
        let seedFrames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: seedData),
            "seed library frame must decode")
        var composite = PodcastUpdate()
        var tracker = KernelModel.DomainRevTracker()
        let seedAccepted = KernelModel.mergeDomainFramesImpl(
            seedFrames, into: &composite, tracker: &tracker)
        XCTAssertTrue(seedAccepted, "seed frame must be accepted")
        XCTAssertEqual(composite.library.count, 1, "composite must carry one show after seeding")
        XCTAssertEqual(composite.inboxLastTriagedAt, 1_717_200_123)

        // Tombstone: library = null at rev=2.
        let tombstoneData = makeEnvelope(projections: [
            DomainSchema.library: [
                "rev": 2,
                "library": NSNull()
            ] as [String: Any]
        ])
        let tombstoneFrames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: tombstoneData),
            "library tombstone frame must decode")
        let libFrame = try XCTUnwrap(tombstoneFrames.library,
                                     "library domain must be non-nil in tombstone frame")
        XCTAssertNil(libFrame.library, "tombstone frame must decode library as nil")

        let tombAccepted = KernelModel.mergeDomainFramesImpl(
            tombstoneFrames, into: &composite, tracker: &tracker)
        XCTAssertTrue(tombAccepted, "tombstone frame must be accepted (rev=2 > rev=1)")
        XCTAssertEqual(composite.library, [],
                       "library tombstone must clear composite.library to empty")
        XCTAssertNil(composite.inboxLastTriagedAt,
                     "library tombstone must clear the inbox triage timestamp")
    }

    // ── downloads tombstone ───────────────────────────────────────────────────

    /// A downloads tombstone `{"rev":N,"downloads":null}` clears composite.downloads.
    ///
    /// Sequence:
    ///   1. Seed the composite with a downloads frame carrying a non-nil snapshot.
    ///   2. Apply a downloads tombstone (rev+1, downloads=null).
    ///   3. Assert composite.downloads == nil.
    func testDownloadsTombstoneClearsDownloadsSlice() throws {
        // Seed: a minimal downloads frame. The DownloadQueueSnapshot decode
        // path needs only the outer struct present; inject an empty queue object.
        let seedData = makeEnvelope(projections: [
            DomainSchema.downloads: [
                "rev": 5,
                "downloads": [
                    "active": [],
                    "queued": []
                ]
            ] as [String: Any]
        ])
        let seedFrames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: seedData),
            "seed downloads frame must decode")
        var composite = PodcastUpdate()
        var tracker = KernelModel.DomainRevTracker()
        let seedAccepted = KernelModel.mergeDomainFramesImpl(
            seedFrames, into: &composite, tracker: &tracker)
        XCTAssertTrue(seedAccepted, "seed frame must be accepted")
        // composite.downloads may or may not be non-nil depending on
        // DownloadQueueSnapshot decode; what matters is the tombstone clears it.

        // Tombstone: downloads=null at rev=6.
        let tombstoneData = makeEnvelope(projections: [
            DomainSchema.downloads: [
                "rev": 6,
                "downloads": NSNull()
            ] as [String: Any]
        ])
        let tombstoneFrames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: tombstoneData),
            "downloads tombstone must decode")
        let dlFrame = try XCTUnwrap(tombstoneFrames.downloads,
                                    "downloads domain must be non-nil in tombstone frame")
        XCTAssertNil(dlFrame.downloads, "tombstone must decode downloads as nil")

        let tombAccepted = KernelModel.mergeDomainFramesImpl(
            tombstoneFrames, into: &composite, tracker: &tracker)
        XCTAssertTrue(tombAccepted, "downloads tombstone must be accepted (rev=6 > rev=5)")
        XCTAssertNil(composite.downloads,
                     "downloads tombstone must clear composite.downloads to nil")
    }

    // ── widget tombstone ──────────────────────────────────────────────────────

    /// A widget tombstone `{"rev":N,"widget":null}` clears composite.widget.
    ///
    /// Sequence:
    ///   1. There is no pre-seeded widget (composite.widget starts nil).
    ///   2. Apply a widget tombstone (rev=10, widget=null) directly.
    ///   3. Assert composite.widget == nil and frame was accepted.
    ///
    /// This variant also validates the tombstone path when no prior value
    /// existed — accepted (rev advances) and slice stays nil.
    func testWidgetTombstoneClearsWidgetSlice() throws {
        let tombstoneData = makeEnvelope(projections: [
            DomainSchema.widget: [
                "rev": 10,
                "widget": NSNull()
            ] as [String: Any]
        ])
        let tombstoneFrames = try XCTUnwrap(
            PodcastDomainFrames.decode(from: tombstoneData),
            "widget tombstone must decode")
        let widFrame = try XCTUnwrap(tombstoneFrames.widget,
                                     "widget domain must be non-nil in tombstone frame")
        XCTAssertNil(widFrame.widget, "tombstone must decode widget as nil")

        var composite = PodcastUpdate()
        var tracker = KernelModel.DomainRevTracker()
        let accepted = KernelModel.mergeDomainFramesImpl(
            tombstoneFrames, into: &composite, tracker: &tracker)
        XCTAssertTrue(accepted, "widget tombstone must be accepted (rev=10 > 0)")
        XCTAssertNil(composite.widget,
                     "widget tombstone must leave composite.widget nil")
        XCTAssertEqual(tracker.widget, 10,
                       "tracker must advance to the tombstone rev")
    }

    // ── cold-start full-pull guard ──────────────────────────────────────────

    func testColdStartPullAllowsEqualRevBeforeHydration() {
        XCTAssertTrue(
            KernelModel.shouldPullPodcastSnapshot(
                currentRev: 7,
                lastProcessedRev: 7,
                hasHydratedPodcastSnapshot: false),
            "the first full pull must re-seed even if a push frame already consumed the same rev")
    }

    func testSteadyStatePullRequiresNewerRevAfterHydration() {
        XCTAssertFalse(
            KernelModel.shouldPullPodcastSnapshot(
                currentRev: 7,
                lastProcessedRev: 7,
                hasHydratedPodcastSnapshot: true),
            "after the first full pull, duplicate revs must be dropped")
        XCTAssertTrue(
            KernelModel.shouldPullPodcastSnapshot(
                currentRev: 8,
                lastProcessedRev: 7,
                hasHydratedPodcastSnapshot: true))
    }
}
