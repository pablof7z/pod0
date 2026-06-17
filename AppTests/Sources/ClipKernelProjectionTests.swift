import XCTest
@testable import Podcastr

// MARK: - ClipKernelProjectionTests
//
// Locks the read-side inversion added in the clips→kernel arc (SLICE 3b fix):
// kernel-owned `ClipSummary` rows (AutoSnip captures + clips persisted across
// restart) must surface in `state.clips` so the Clippings UI renders them.
// Without this projection, an AutoSnip dispatch creates+persists the clip in
// the kernel but the iOS UI never sees it (a silent no-op).
//
// The merge is intentionally NOT a blind SET: the kernel `ClipSummary` is
// lossy relative to the domain `Clip` (no transcriptText/speakerID/source),
// and the in-app composer builds rich local clips that are persisted
// Swift-side and not dispatched to the kernel. These tests pin both halves:
// kernel clips appear, and local rich clips are preserved.

@MainActor
final class ClipKernelProjectionTests: XCTestCase {

    private var storeFileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        if let url = storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: url)
        }
        store = nil
        storeFileURL = nil
        try await super.tearDown()
    }

    // MARK: - Helpers

    private func makeSummary(
        id: UUID = UUID(),
        episodeID: UUID,
        startSecs: Double = 30,
        endSecs: Double = 90,
        title: String? = nil,
        createdAt: Int = 1_700_000_000
    ) -> ClipSummary {
        ClipSummary(
            id: id.uuidString,
            episodeId: episodeID.uuidString,
            episodeTitle: "Episode Title",
            podcastTitle: "Podcast Title",
            startSecs: startSecs,
            endSecs: endSecs,
            title: title,
            createdAt: createdAt
        )
    }

    // MARK: - Kernel clip surfaces

    func testKernelClipProjectsIntoStateClips() {
        let episodeID = UUID()
        let podcastID = UUID()
        let clipID = UUID()
        let summary = makeSummary(id: clipID, episodeID: episodeID, title: "A moment")

        store.projectKernelClips([summary], episodeToPodcast: [episodeID: podcastID])

        XCTAssertEqual(store.state.clips.count, 1)
        let clip = store.state.clips[0]
        XCTAssertEqual(clip.id, clipID)
        XCTAssertEqual(clip.episodeID, episodeID)
        XCTAssertEqual(clip.subscriptionID, podcastID, "subscriptionID resolves from episode→podcast map")
        XCTAssertEqual(clip.startMs, 30_000)
        XCTAssertEqual(clip.endMs, 90_000)
        XCTAssertEqual(clip.caption, "A moment", "kernel title maps to caption")
        XCTAssertEqual(clip.source, .auto, "kernel-owned clips default to .auto")
    }

    func testUnknownEpisodeFallsBackToUnknownSubscription() {
        let episodeID = UUID()
        let summary = makeSummary(episodeID: episodeID)

        // Empty map → episode not in any known podcast.
        store.projectKernelClips([summary], episodeToPodcast: [:])

        XCTAssertEqual(store.state.clips.count, 1)
        XCTAssertEqual(store.state.clips[0].subscriptionID, Podcast.unknownID)
    }

    // MARK: - SET semantics (kernel authoritative for its clips)

    func testProjectionIsIdempotentBySetSemantics() {
        let episodeID = UUID()
        let podcastID = UUID()
        let summary = makeSummary(episodeID: episodeID)

        store.projectKernelClips([summary], episodeToPodcast: [episodeID: podcastID])
        store.projectKernelClips([summary], episodeToPodcast: [episodeID: podcastID])

        XCTAssertEqual(store.state.clips.count, 1, "re-projecting the same kernel clip does not duplicate it")
    }

    func testKernelClipRemovalReflectsWhenNoLocalCounterpart() {
        let episodeID = UUID()
        let podcastID = UUID()
        let summary = makeSummary(episodeID: episodeID)

        store.projectKernelClips([summary], episodeToPodcast: [episodeID: podcastID])
        XCTAssertEqual(store.state.clips.count, 1)

        // Kernel no longer reports the clip (deleted) → projection drops it,
        // since there is no local-only counterpart to preserve.
        store.projectKernelClips([], episodeToPodcast: [episodeID: podcastID])
        XCTAssertTrue(store.state.clips.isEmpty)
    }

    // MARK: - Local rich clips are preserved

    func testLocalOnlyClipSurvivesProjection() {
        let episodeID = UUID()
        let podcastID = UUID()
        // Composer-authored clip with rich data, never dispatched to the kernel.
        let local = Clip(
            episodeID: episodeID,
            subscriptionID: podcastID,
            startMs: 1_000,
            endMs: 5_000,
            caption: "Composed",
            speakerID: UUID().uuidString,
            transcriptText: "verbatim quote",
            source: .touch
        )
        store.addClip(local)

        // Kernel reports a DIFFERENT clip; local one must not be wiped.
        let kernelSummary = makeSummary(episodeID: episodeID)
        store.projectKernelClips([kernelSummary], episodeToPodcast: [episodeID: podcastID])

        XCTAssertEqual(store.state.clips.count, 2)
        let preserved = store.state.clips.first { $0.id == local.id }
        XCTAssertNotNil(preserved)
        XCTAssertEqual(preserved?.transcriptText, "verbatim quote", "rich local data preserved")
        XCTAssertEqual(preserved?.source, .touch)
    }

    func testLocalRichVersionWinsOverKernelForSameID() {
        let episodeID = UUID()
        let podcastID = UUID()
        let sharedID = UUID()
        // A local clip whose id also appears in the kernel projection: the
        // local (richer) version wins so transcript/speaker/source aren't lost.
        let local = Clip(
            id: sharedID,
            episodeID: episodeID,
            subscriptionID: podcastID,
            startMs: 2_000,
            endMs: 8_000,
            caption: "Local caption",
            speakerID: nil,
            transcriptText: "local transcript",
            source: .touch
        )
        store.addClip(local)

        let kernelSummary = makeSummary(
            id: sharedID, episodeID: episodeID, title: "Kernel title")
        store.projectKernelClips([kernelSummary], episodeToPodcast: [episodeID: podcastID])

        XCTAssertEqual(store.state.clips.count, 1, "same id is not duplicated")
        let clip = store.state.clips[0]
        XCTAssertEqual(clip.transcriptText, "local transcript", "local rich version wins")
        XCTAssertEqual(clip.source, .touch)
    }

    // MARK: - Ordering

    func testProjectedClipsAreNewestFirst() {
        let episodeID = UUID()
        let podcastID = UUID()
        let older = makeSummary(episodeID: episodeID, createdAt: 1_000)
        let newer = makeSummary(episodeID: episodeID, createdAt: 2_000)

        store.projectKernelClips([older, newer], episodeToPodcast: [episodeID: podcastID])

        XCTAssertEqual(store.state.clips.count, 2)
        XCTAssertGreaterThan(
            store.state.clips[0].createdAt,
            store.state.clips[1].createdAt,
            "clips sorted newest-first to match ClippingsView")
    }
}
