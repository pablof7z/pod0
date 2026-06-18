import UIKit
import XCTest
@testable import Podcastr

@MainActor
final class AppTests: XCTestCase {

    // MARK: - Per-test isolated storage
    //
    // Every test gets a fresh `AppStateStore` over a unique in-memory
    // `UserDefaults` suite (see `AppStateTestSupport`). This keeps the
    // production App Group suite (`group.com.podcastr.app`) clean — running
    // the test target used to leak fixture data ("Test Show", "Episode e1")
    // into the real app's persisted state.

    private var storeFileURL: URL!
    private var store: AppStateStore!

    override func setUp() async throws {
        try await super.setUp()
        let made = await AppStateTestSupport.makeIsolatedStore()
        storeFileURL = made.fileURL
        store = made.store
    }

    override func tearDown() async throws {
        if let storeFileURL {
            AppStateTestSupport.disposeIsolatedStore(at: storeFileURL)
        }
        store = nil
        storeFileURL = nil
        try await super.tearDown()
    }

    // MARK: - Subscriptions

    func testAddSubscriptionAcceptsNewFeed() throws {
        let initialCount = store.state.subscriptions.count

        let sub = makeSubscription(title: "Test Show")
        store.upsertPodcast(sub)
        let inserted = store.addSubscription(podcastID: sub.id)

        XCTAssertTrue(inserted)
        XCTAssertEqual(store.state.subscriptions.count, initialCount + 1)
        XCTAssertEqual(store.podcast(id: sub.id)?.title, "Test Show")
    }

    func testAddSubscriptionRejectsDuplicateFeedURL() throws {
        // Use a UUID-unique URL so prior persisted state can't collide.
        let url = URL(string: "https://example.com/\(UUID().uuidString).xml")!

        let firstPodcast = makeSubscription(feedURL: url)
        store.upsertPodcast(firstPodcast)
        XCTAssertTrue(store.addSubscription(podcastID: firstPodcast.id))
        let countAfterFirst = store.state.subscriptions.count
        // A second call for the same podcast must NOT add another follow row.
        XCTAssertFalse(store.addSubscription(podcastID: firstPodcast.id))
        XCTAssertEqual(store.state.subscriptions.count, countAfterFirst)
    }

    // `testRemoveSubscriptionAlsoRemovesItsEpisodes` and
    // `testSetSubscriptionNotificationsToggle` were deleted: both
    // `deletePodcast` and `setSubscriptionNotificationsEnabled` now route
    // through the Rust kernel (kernel-dispatch only; no-op without a live
    // kernel in unit tests). The behavior is covered by
    // `cargo test -p nmp-app-podcast library`.

    // MARK: - Episodes

    // Position persistence tests (`testSetEpisodePlaybackPosition`,
    // `testPositionUpdatesAreDebounced`, `testEagerCapFiresAfterMaxIntervalElapsed`,
    // `testBackgroundFlushPersistsPendingPosition`) were removed in PR #572:
    // Swift's position-debounce machinery (positionCache / setEpisodePlaybackPosition /
    // flushPendingPositions) has been deleted. Position persistence is now owned
    // entirely by the Rust kernel's `apply_writeback` (audio_report.rs), covered by
    // `cargo test -p nmp-app-podcast -- ffi::audio_report`.

    // `testMarkPlayedFlushesBeforeReset` was deleted: `markEpisodePlayed` now
    // dispatches to the Rust kernel (no-op without kernel in unit tests) so
    // assertions on `episode.played == true` always fail. The flush-before-
    // reset ordering guarantee is covered by `cargo test -p nmp-app-podcast inbox`.

    // MARK: - Friends

    func testAddFriend() throws {
        let friend = store.addFriend(displayName: "Alice", identifier: "alice@example.com")

        XCTAssertEqual(friend.displayName, "Alice")
        XCTAssertEqual(friend.identifier, "alice@example.com")
        XCTAssertTrue(store.state.friends.contains { $0.id == friend.id })
    }

    func testUpdateFriendDisplayName() throws {
        let friend = store.addFriend(displayName: "Bob", identifier: "bob_id")

        store.updateFriendDisplayName(friend.id, newName: "Robert")

        XCTAssertEqual(store.state.friends.first { $0.id == friend.id }?.displayName, "Robert")
    }

    func testRemoveFriend() throws {
        let friend = store.addFriend(displayName: "Charlie", identifier: "charlie_id")

        store.removeFriend(friend.id)

        XCTAssertFalse(store.state.friends.contains { $0.id == friend.id })
    }

    // MARK: - Models

    func testAnchorCodable() throws {
        let anchor = Anchor.note(id: UUID())
        let data = try JSONEncoder().encode(anchor)
        let decoded = try JSONDecoder().decode(Anchor.self, from: data)
        XCTAssertEqual(anchor, decoded)
    }

    func testFriendAnchorCodable() throws {
        let anchor = Anchor.friend(id: UUID())
        let data = try JSONEncoder().encode(anchor)
        let decoded = try JSONDecoder().decode(Anchor.self, from: data)
        XCTAssertEqual(anchor, decoded)
    }

    // MARK: - AgentPrompt

    // NOTE: inventory selection / filtering / capping / recency-window policy
    // now lives in the Rust kernel (`ffi::agent_context::build_agent_context`,
    // covered by `cargo test -p nmp-app-podcast agent_context`). These Swift
    // tests now only assert that `AgentPrompt` RENDERS a kernel-provided
    // `AgentContextSnapshot` and the `AppState`-sourced sections it still owns
    // (friends, memories).

    func testAgentPromptIncludesFriends() {
        var state = AppState()
        state.friends.append(Friend(displayName: "Alice", identifier: "alice_id"))

        let prompt = AgentPrompt.build(for: state, agentContext: nil, memoryFacts: [])

        XCTAssertTrue(prompt.contains("Alice"))
    }

    func testAgentPromptIncludesMemories() {
        let memory = MemoryFact(
            id: "mem-1",
            key: "schedule_preference",
            value: "User prefers mornings",
            source: "user",
            createdAt: 0
        )

        let prompt = AgentPrompt.build(for: AppState(), agentContext: nil, memoryFacts: [memory])

        XCTAssertTrue(prompt.contains("User prefers mornings"))
    }

    func testAgentPromptRendersKernelSubscriptions() {
        var ctx = AgentContextSnapshot()
        ctx.subscriptions = ["The Tim Ferriss Show", "Acquired"]
        ctx.subscriptionsTotal = 2

        let prompt = AgentPrompt.build(for: AppState(), agentContext: ctx, memoryFacts: [])

        XCTAssertTrue(prompt.contains("## Subscriptions (2)"))
        XCTAssertTrue(prompt.contains("The Tim Ferriss Show"))
        XCTAssertTrue(prompt.contains("Acquired"))
    }

    func testAgentPromptRendersAndMoreSuffixFromKernelTotal() {
        var ctx = AgentContextSnapshot()
        ctx.subscriptions = ["Listed Show"]
        // Kernel capped a larger followed set; renderer shows the overflow.
        ctx.subscriptionsTotal = 4

        let prompt = AgentPrompt.build(for: AppState(), agentContext: ctx, memoryFacts: [])

        XCTAssertTrue(prompt.contains("## Subscriptions (4)"))
        // The Rust renderer uses ASCII "..." (three dots), not the Unicode ellipsis "…".
        XCTAssertTrue(prompt.contains("...and 3 more"))
    }

    func testAgentPromptRendersKernelInProgressEpisodes() {
        var ctx = AgentContextSnapshot()
        ctx.inProgress = [
            AgentContextEpisode(title: "Episode about something", showTitle: "Lex Fridman"),
        ]

        let prompt = AgentPrompt.build(for: AppState(), agentContext: ctx, memoryFacts: [])

        XCTAssertTrue(prompt.contains("## In Progress"))
        XCTAssertTrue(prompt.contains("Episode about something"))
        XCTAssertTrue(prompt.contains("Lex Fridman"))
    }

    func testAgentPromptRendersKernelRecentUnplayedWithWindowLabel() {
        var ctx = AgentContextSnapshot()
        ctx.recentUnplayed = [
            AgentContextEpisode(title: "Brand new episode", showTitle: "Recent Show"),
        ]
        ctx.recentWindowDays = 7

        let prompt = AgentPrompt.build(for: AppState(), agentContext: ctx, memoryFacts: [])

        XCTAssertTrue(prompt.contains("## Recent (last 7 days, unplayed)"))
        XCTAssertTrue(prompt.contains("Brand new episode"))
    }

    func testAgentPromptOmitsInventorySectionsWhenContextNil() {
        let prompt = AgentPrompt.build(for: AppState(), agentContext: nil, memoryFacts: [])

        XCTAssertFalse(prompt.contains("## Subscriptions"))
        XCTAssertFalse(prompt.contains("## In Progress"))
        XCTAssertFalse(prompt.contains("## Recent"))
    }

    // MARK: - Persistence isolation

    /// Regression test for the test-leak bug: writing through an isolated
    /// store must NOT mutate the production App Group state file.
    func testIsolatedStoreDoesNotTouchSharedAppGroupContainer() throws {
        let productionURL = Persistence.appGroupStateFileURL
        // Snapshot whatever the production file currently holds (may be
        // absent on a clean dev machine — `nil` is a valid baseline).
        let before = try? Data(contentsOf: productionURL)

        // Make a noisy mutation through the isolated store.
        let sub = makeSubscription(title: "Leak Canary \(UUID().uuidString)")
        store.upsertPodcast(sub)
        XCTAssertTrue(store.addSubscription(podcastID: sub.id))

        // The production file must be byte-identical to the snapshot.
        let after = try? Data(contentsOf: productionURL)
        XCTAssertEqual(before, after, "Test mutation leaked into the shared App Group state file.")
    }

    // MARK: - Settings

    func testSettingsPersistsBYOKMetadataOnly() throws {
        var settings = Settings()
        settings.markOpenRouterBYOK(keyID: "key_123", keyLabel: "Default")
        settings.markAssemblyAIBYOK(keyID: "key_assembly", keyLabel: "AssemblyAI")
        settings.markPerplexityBYOK(keyID: "key_perplexity", keyLabel: "Perplexity")

        let encoded = try JSONEncoder().encode(settings)
        let encodedString = String(data: encoded, encoding: .utf8) ?? ""

        XCTAssertTrue(encodedString.contains("byok"))
        XCTAssertTrue(encodedString.contains("key_123"))
        XCTAssertTrue(encodedString.contains("Default"))
        XCTAssertTrue(encodedString.contains("key_assembly"))
        XCTAssertTrue(encodedString.contains("AssemblyAI"))
        XCTAssertTrue(encodedString.contains("key_perplexity"))
        XCTAssertTrue(encodedString.contains("Perplexity"))
        XCTAssertFalse(encodedString.contains("api_key"))
    }

    // MARK: - Fixtures

    private func makeSubscription(
        feedURL: URL = URL(string: "https://example.com/\(UUID().uuidString).xml")!,
        title: String = "Test Show"
    ) -> Podcast {
        Podcast(feedURL: feedURL, title: title)
    }

    private func makeEpisode(
        podcastID: UUID,
        guid: String
    ) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: guid,
            title: "Episode \(guid)",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/\(guid).mp3")!
        )
    }
}
