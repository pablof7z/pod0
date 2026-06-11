import XCTest
@testable import Podcastr

/// Pins the kernel-owned widget channel (D4): the `WidgetSnapshot` wire shape
/// the app encodes and the change-gated / fraction-quantized write cadence
/// `PlatformCapability` applies. The widget extension decodes the exact JSON
/// `writeWidgetSnapshot` returns, so the snake_case key round-trip is the
/// contract.
@MainActor
final class PlatformWidgetSnapshotTests: XCTestCase {

    private func sample(
        episode: String? = "Episode One",
        fraction: Float = 0.25,
        position: Double = 30,
        duration: Double = 120,
        playing: Bool = true,
        unplayed: Int = 3
    ) -> WidgetSnapshot {
        WidgetSnapshot(
            nowPlayingEpisodeTitle: episode,
            nowPlayingPodcastTitle: "Great Show",
            nowPlayingArtworkURL: "https://ex.com/a.png",
            nowPlayingChapterTitle: "Chapter 2",
            isPlaying: playing,
            positionFraction: fraction,
            positionSecs: position,
            durationSecs: duration,
            unplayedCount: unplayed)
    }

    // MARK: - Wire shape

    func testWriteWidgetSnapshotEncodesSnakeCaseWireShape() throws {
        let cap = PlatformCapability()
        let data = try XCTUnwrap(cap.writeWidgetSnapshot(sample()))
        let json = try XCTUnwrap(String(data: data, encoding: .utf8))
        XCTAssertTrue(json.contains("\"now_playing_episode_title\""))
        XCTAssertTrue(json.contains("\"now_playing_podcast_title\""))
        XCTAssertTrue(json.contains("\"now_playing_artwork_url\""))
        XCTAssertTrue(json.contains("\"now_playing_chapter_title\""))
        XCTAssertTrue(json.contains("\"is_playing\""))
        XCTAssertTrue(json.contains("\"position_fraction\""))
        XCTAssertTrue(json.contains("\"position_secs\""))
        XCTAssertTrue(json.contains("\"duration_secs\""))
        XCTAssertTrue(json.contains("\"unplayed_count\""))

        // The bytes round-trip back through the same CodingKeys the widget
        // extension uses.
        let decoded = try JSONDecoder().decode(WidgetSnapshot.self, from: data)
        XCTAssertEqual(decoded, sample())
    }

    func testWidgetSnapshotDecodesFromKernelStyleSnakeCaseJSON() throws {
        let json = """
        {"now_playing_episode_title":"Ep","now_playing_podcast_title":"Show",
        "now_playing_artwork_url":"https://x/a.png","now_playing_chapter_title":"Ch 1",
        "is_playing":true,"position_fraction":0.5,"position_secs":60.0,
        "duration_secs":120.0,"unplayed_count":4}
        """
        let decoded = try JSONDecoder().decode(WidgetSnapshot.self, from: Data(json.utf8))
        XCTAssertEqual(decoded.nowPlayingEpisodeTitle, "Ep")
        XCTAssertEqual(decoded.nowPlayingChapterTitle, "Ch 1")
        XCTAssertEqual(decoded.positionFraction, 0.5)
        XCTAssertEqual(decoded.durationSecs, 120.0)
        XCTAssertEqual(decoded.unplayedCount, 4)
    }

    // MARK: - Cadence (change-gating + fraction quantization)

    func testApplyWidgetSnapshotIsChangeGated() {
        let cap = PlatformCapability()
        // Distinct snapshots write; an identical follow-up does not.
        XCTAssertTrue(cap.applyWidgetSnapshot(update(with: sample())))
        XCTAssertFalse(cap.applyWidgetSnapshot(update(with: sample())))
        // A metadata change (episode title) writes again.
        XCTAssertTrue(
            cap.applyWidgetSnapshot(update(with: sample(episode: "Episode Two"))))
    }

    func testSubOnePercentFractionDriftDoesNotWrite() {
        let cap = PlatformCapability()
        XCTAssertTrue(cap.applyWidgetSnapshot(update(with: sample(fraction: 0.250))))
        // +0.4% — same 1% bucket → no write.
        XCTAssertFalse(cap.applyWidgetSnapshot(update(with: sample(fraction: 0.254))))
        // Crossing into the next 1% bucket → writes.
        XCTAssertTrue(cap.applyWidgetSnapshot(update(with: sample(fraction: 0.262))))
    }

    func testNilWidgetClearsOnceThenNoOps() {
        let cap = PlatformCapability()
        XCTAssertTrue(cap.applyWidgetSnapshot(update(with: sample())))
        // First nil clears (returns true = state changed); subsequent nils no-op.
        XCTAssertTrue(cap.applyWidgetSnapshot(PodcastUpdate()))
        XCTAssertFalse(cap.applyWidgetSnapshot(PodcastUpdate()))
    }

    private func update(with widget: WidgetSnapshot) -> PodcastUpdate {
        var u = PodcastUpdate()
        u.widget = widget
        return u
    }
}
