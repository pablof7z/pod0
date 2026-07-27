import Foundation
import XCTest
@testable import Podcastr

final class PlayerProgressPresentationTests: XCTestCase {
    func testPlayerUsesFullScreenPresentationWithExplicitDismiss() throws {
        let root = repositoryRoot()
        let rootSource = try String(
            contentsOf: root.appendingPathComponent("App/Sources/App/RootView.swift"),
            encoding: .utf8
        )
        let topBarSource = try String(
            contentsOf: root.appendingPathComponent(
                "App/Sources/Features/Player/PlayerTopBar.swift"
            ),
            encoding: .utf8
        )

        XCTAssertTrue(rootSource.contains(".fullScreenCover(isPresented: $showFullPlayer)"))
        XCTAssertFalse(rootSource.contains(".sheet(isPresented: $showFullPlayer)"))
        XCTAssertTrue(topBarSource.contains(".accessibilityLabel(\"Close player\")"))
        XCTAssertTrue(topBarSource.contains("onDismiss()"))
    }

    func testPersistentPlayerButtonsHaveNoRenderedGlassContainers() throws {
        let root = repositoryRoot()
        let paths = [
            "App/Sources/Features/Player/PlayerTopBar.swift",
            "App/Sources/Features/Player/PlayerControlsView.swift",
            "App/Sources/Features/Player/PlayerPlaybackChrome.swift",
            "App/Sources/Features/Player/PlayerMoreMenu.swift",
            "App/Sources/Features/Player/PlayerPrerollSkipButton.swift",
            "App/Sources/Features/Player/PlayerClipSourceChip.swift",
            "App/Sources/Features/Player/PlayerGenerationSourceChip.swift",
            "App/Sources/Features/Player/AutoSnip/AutoSnipButton.swift",
        ]

        for path in paths {
            let source = try String(
                contentsOf: root.appendingPathComponent(path),
                encoding: .utf8
            )
            XCTAssertFalse(source.contains(".glassEffect("), path)
            XCTAssertFalse(source.contains(".glassSurface("), path)
            XCTAssertFalse(source.contains("GlassEffectContainer("), path)
        }
    }

    func testApproximateDurationUsesCompactWholeUnits() {
        XCTAssertEqual(PlayerTimeFormat.approximateDuration(30), "1m")
        XCTAssertEqual(PlayerTimeFormat.approximateDuration(60), "1m")
        XCTAssertEqual(PlayerTimeFormat.approximateDuration(50 * 60), "50m")
        XCTAssertEqual(PlayerTimeFormat.approximateDuration(65 * 60), "1h 5m")
        XCTAssertNil(PlayerTimeFormat.approximateDuration(0))
    }

    func testChapterDurationUsesNextChapterAndEpisodeEnd() {
        let chapters = [
            Episode.Chapter(startTime: 0, title: "Opening"),
            Episode.Chapter(startTime: 90, title: "Middle"),
        ]

        XCTAssertEqual(
            PlayerChapterPresentation.duration(
                for: chapters[0],
                in: chapters,
                episodeDuration: 300
            ),
            90
        )
        XCTAssertEqual(
            PlayerChapterPresentation.duration(
                for: chapters[1],
                in: chapters,
                episodeDuration: 300
            ),
            210
        )
    }

    func testChapterDurationHonorsEarlierExplicitEnd() {
        let chapter = Episode.Chapter(
            startTime: 10,
            endTime: 40,
            title: "Short chapter"
        )
        let chapters = [
            chapter,
            Episode.Chapter(startTime: 60, title: "Next"),
        ]

        XCTAssertEqual(
            PlayerChapterPresentation.duration(
                for: chapter,
                in: chapters,
                episodeDuration: 300
            ),
            30
        )
    }

    func testChapterProgressClampsBeforeDuringAndAfter() {
        let chapter = Episode.Chapter(startTime: 60, endTime: 120, title: "Chapter")
        let chapters = [chapter]

        XCTAssertEqual(
            PlayerChapterPresentation.progress(
                for: chapter,
                in: chapters,
                episodeDuration: 180,
                currentTime: 30
            ),
            0
        )
        XCTAssertEqual(
            PlayerChapterPresentation.progress(
                for: chapter,
                in: chapters,
                episodeDuration: 180,
                currentTime: 90
            ),
            0.5,
            accuracy: 0.001
        )
        XCTAssertEqual(
            PlayerChapterPresentation.progress(
                for: chapter,
                in: chapters,
                episodeDuration: 180,
                currentTime: 150
            ),
            1
        )
    }

    private func repositoryRoot() -> URL {
        URL(fileURLWithPath: #filePath)
            .deletingLastPathComponent()
            .deletingLastPathComponent()
            .deletingLastPathComponent()
    }
}

@MainActor
final class PlaybackSeekHistoryTests: XCTestCase {
    func testJumpBackOffersTemporaryForwardAndForwardRestoresBack() throws {
        let episode = makeEpisode()
        let state = PlaybackState()
        state.episode = episode
        state.engine.setCurrentTime(120)
        state.seekHistory = [
            SeekHistoryEntry(episodeID: episode.id, position: 30, episode: episode),
        ]

        state.jumpBack()

        XCTAssertFalse(state.canJumpBack)
        XCTAssertTrue(state.canJumpForward)
        XCTAssertEqual(
            try XCTUnwrap(state.jumpForwardEntry).position,
            120,
            accuracy: 0.001
        )

        state.jumpForward()

        XCTAssertFalse(state.canJumpForward)
        XCTAssertTrue(state.canJumpBack)
        state.jumpForwardExpiryTask?.cancel()
    }

    func testNewNavigationClearsForwardOffer() {
        let episode = makeEpisode()
        let state = PlaybackState()
        state.episode = episode
        state.jumpForwardEntry = SeekHistoryEntry(
            episodeID: episode.id,
            position: 120,
            episode: episode
        )

        state.navigationalSeek(to: 60)

        XCTAssertFalse(state.canJumpForward)
    }

    private func makeEpisode() -> Episode {
        let id = UUID()
        return Episode(
            id: id,
            podcastID: UUID(),
            guid: "seek-history-\(id.uuidString)",
            title: "Seek History",
            pubDate: Date(),
            duration: 300,
            enclosureURL: URL(string: "https://example.com/\(id.uuidString).mp3")!
        )
    }
}
