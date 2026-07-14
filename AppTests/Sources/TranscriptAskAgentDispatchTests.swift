import XCTest
@testable import Podcastr

/// Coverage for the long-press → "Ask the agent" wiring.
///
/// We can't drive `PlayerTranscriptScrollView`'s `.contextMenu` from a unit
/// test (SwiftUI views aren't introspectable here), but the round-trip splits
/// cleanly into two pure pieces we *can* pin:
///
///  1. `TranscriptAgentContext.prefilledDraft` — formatting contract.
///  2. `AskAgentDispatcher.dispatch` — store mutation + notification post.
///
/// `AgentChatSession`'s drain (read store field, clear, expose via
/// `consumeSeededDraft`) is four lines of straight code that depends on
/// `ChatHistoryStore`'s shared Documents-dir backing — not worth the plumbing
/// for a regression test. The dispatcher+formatter tests guarantee the
/// session sees the right input.
@MainActor
final class TranscriptAskAgentDispatchTests: XCTestCase {

    private var fileURL: URL!

    override func tearDown() {
        if let fileURL { AppStateTestSupport.disposeIsolatedStore(at: fileURL) }
        fileURL = nil
        super.tearDown()
    }

    // MARK: - Prefill formatter

    func testPrefilledDraftIncludesShowTimestampAndQuotedSegment() {
        let context = TranscriptAgentContext(
            episodeID: UUID(),
            subscriptionTitle: "All-In",
            segmentText: "  The keto diet is bullshit.  ",
            timestamp: 754   // 12:34
        )
        let draft = context.prefilledDraft
        XCTAssertTrue(draft.contains("All-In"))
        XCTAssertTrue(draft.contains("12:34"))
        // Trim around the quoted segment so the blockquote stays clean.
        XCTAssertTrue(draft.contains("> The keto diet is bullshit."))
        XCTAssertFalse(draft.contains(">   The keto"), "leading whitespace should be trimmed")
    }

    func testPrefilledDraftFallsBackWhenShowTitleMissing() {
        let context = TranscriptAgentContext(
            episodeID: UUID(),
            subscriptionTitle: "",
            segmentText: "Anything.",
            timestamp: 0
        )
        XCTAssertTrue(context.prefilledDraft.contains("this episode"))
        XCTAssertTrue(context.prefilledDraft.contains("0:00"))
    }

    func testPrefilledDraftFormatsTimestampWithLeadingZero() {
        let context = TranscriptAgentContext(
            episodeID: UUID(),
            subscriptionTitle: "Show",
            segmentText: "x",
            timestamp: 65   // 1:05, not 1:5
        )
        XCTAssertTrue(context.prefilledDraft.contains("1:05"))
    }

    // MARK: - Dispatcher

    func testDispatchWritesContextOnStoreAndPostsNotification() {
        let result = AppStateTestSupport.makeIsolatedStore()
        fileURL = result.fileURL
        let store = result.store

        let subID = UUID()
        let subscription = Podcast(
            id: subID,
            feedURL: URL(string: "https://example.com/feed.xml")!,
            title: "Test Show"
        )
        store.state.podcasts = [subscription]
        store.state.subscriptions = [PodcastSubscription(podcastID: subscription.id)]

        let episode = Episode(
            podcastID: subID,
            guid: "guid-1",
            title: "Test Episode",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
        let segment = Segment(start: 95, end: 100, speakerID: nil, text: "Hello world.")

        let expectation = XCTestExpectation(description: "askAgentRequested fires")
        let token = NotificationCenter.default.addObserver(
            forName: .askAgentRequested,
            object: nil,
            queue: .main
        ) { _ in expectation.fulfill() }
        defer { NotificationCenter.default.removeObserver(token) }

        AskAgentDispatcher.dispatch(segment: segment, episode: episode, store: store)

        wait(for: [expectation], timeout: 1.0)
        let pending = store.pendingTranscriptAgentContext
        XCTAssertEqual(pending?.episodeID, episode.id)
        XCTAssertEqual(pending?.subscriptionTitle, "Test Show")
        XCTAssertEqual(pending?.segmentText, "Hello world.")
        XCTAssertEqual(pending?.timestamp, 95)
    }

    func testDispatchNoOpsWhenEpisodeMissing() {
        let result = AppStateTestSupport.makeIsolatedStore()
        fileURL = result.fileURL
        let store = result.store

        let segment = Segment(start: 0, end: 1, speakerID: nil, text: "x")
        AskAgentDispatcher.dispatch(segment: segment, episode: nil, store: store)
        XCTAssertNil(store.pendingTranscriptAgentContext)
    }

    func testDispatchUsesEmptyTitleWhenSubscriptionMissing() {
        let result = AppStateTestSupport.makeIsolatedStore()
        fileURL = result.fileURL
        let store = result.store

        let episode = Episode(
            podcastID: UUID(),  // not in store.subscriptions
            guid: "guid-1",
            title: "Test Episode",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/audio.mp3")!
        )
        let segment = Segment(start: 5, end: 10, speakerID: nil, text: "Quote.")

        AskAgentDispatcher.dispatch(segment: segment, episode: episode, store: store)
        // Title falls back to empty; the formatter then says "this episode".
        XCTAssertEqual(store.pendingTranscriptAgentContext?.subscriptionTitle, "")
    }
}
