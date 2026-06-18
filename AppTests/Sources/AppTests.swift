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

    func testSetEpisodePlaybackPosition() throws {
        let sub = makeSubscription()
        store.upsertPodcast(sub)
        store.addSubscription(podcastID: sub.id)
        let ep = makeEpisode(podcastID: sub.id, guid: "e1")
        store.upsertEpisodes([ep], forPodcast: sub.id)

        store.setEpisodePlaybackPosition(ep.id, position: 123.4)

        // Read through the store helper so the assertion stays correct under
        // both the eager-first path (value materialised in `state.episodes`)
        // and the debounced path (value still cached, folded in by `episode(id:)`).
        let live = try XCTUnwrap(store.episode(id: ep.id))
        XCTAssertEqual(live.playbackPosition, 123.4, accuracy: 0.001)
    }

    /// 30 rapid `setEpisodePlaybackPosition` calls (mirroring the
    /// `PlaybackState.tickPersistence` 1 Hz loop) must coalesce into ≤ 2
    /// disk writes: one eager (so the first position is durable immediately
    /// after playback starts) plus one trailing flush after the 5-second
    /// post-activity debounce. The pre-fix code wrote 30 times — 240 MB/min
    /// during a real episode — and pinned the main actor.
    func testPositionUpdatesAreDebounced() async throws {
        let sub = makeSubscription()
        store.upsertPodcast(sub); store.addSubscription(podcastID: sub.id)
        let ep = makeEpisode(podcastID: sub.id, guid: "tick-\(UUID().uuidString)")
        store.upsertEpisodes([ep], forPodcast: sub.id)

        // Only count writes triggered by the test body itself — addSubscription
        // and upsertEpisodes have already saved.
        store.persistence.resetSaveInvocationCount()

        // Tight loop: 30 monotonically-increasing positions, no awaits in
        // between. The eager-first call commits position #1; everything
        // after that should land in the in-memory cache.
        for i in 1...30 {
            store.setEpisodePlaybackPosition(ep.id, position: TimeInterval(i))
        }

        // Immediately after the loop the trailing-flush task is still
        // sleeping. We expect exactly one save (the eager first) at this
        // point.
        XCTAssertLessThanOrEqual(
            store.persistence.saveInvocationCount, 1,
            "Tight position-update loop wrote to disk more than once before the debounce fired."
        )

        // The latest cached position must be readable without waiting for
        // the flush — UI surfaces (`inProgressEpisodes`, the player resume
        // path) cannot tolerate stale reads.
        let live = try XCTUnwrap(store.episode(id: ep.id))
        XCTAssertEqual(live.playbackPosition, 30.0, accuracy: 0.001)

        // Wait long enough for the trailing-debounce task to drain (5s post-
        // activity window plus a comfortable margin for CI jitter).
        try await Task.sleep(for: .seconds(7))

        XCTAssertLessThanOrEqual(
            store.persistence.saveInvocationCount, 2,
            "Position debounce did not coalesce 30 rapid updates into ≤ 2 disk writes."
        )

        // After the trailing flush the on-disk position must match the
        // last cached value — otherwise a crash would silently rewind the
        // user to whatever the eager-first call wrote.
        let reopened = await AppStateTestSupport.makeIsolatedStore(
            fileURL: storeFileURL,
            reset: false
        )
        XCTAssertEqual(
            reopened.store.episode(id: ep.id)?.playbackPosition ?? -1, 30.0,
            accuracy: 0.001,
            "Trailing flush did not persist the latest cached position."
        )
    }

    /// During continuous playback the trailing-debounce task is cancelled
    /// on every tick, so the *only* mechanism that lands the cache on disk
    /// is the 30-second eager-cap. If this gate regresses to always-false,
    /// `testPositionUpdatesAreDebounced` still passes (the trailing flush
    /// fires after the test's tight loop ends), but a real 60-second
    /// crash-loss would silently exceed the spec. This test guards the
    /// cap directly by fast-forwarding `lastPositionFlush`.
    func testEagerCapFiresAfterMaxIntervalElapsed() throws {
        let sub = makeSubscription()
        store.upsertPodcast(sub); store.addSubscription(podcastID: sub.id)
        let ep = makeEpisode(podcastID: sub.id, guid: "cap-\(UUID().uuidString)")
        store.upsertEpisodes([ep], forPodcast: sub.id)

        // Eager-first call lands on disk and stamps `lastPositionFlush`.
        store.setEpisodePlaybackPosition(ep.id, position: 1.0)
        store.persistence.resetSaveInvocationCount()

        // Inside the cap window — must stay cached.
        store.setEpisodePlaybackPosition(ep.id, position: 2.0)
        XCTAssertEqual(
            store.persistence.saveInvocationCount, 0,
            "Position update inside the eager-cap window leaked through to disk."
        )

        // Simulate 31 s of continuous playback since the last flush — past
        // the 30 s cap. The next call must save eagerly even though the
        // trailing-debounce task hasn't expired.
        store.lastPositionFlush = Date().addingTimeInterval(-31)

        store.setEpisodePlaybackPosition(ep.id, position: 3.0)
        XCTAssertEqual(
            store.persistence.saveInvocationCount, 1,
            "Eager-cap did not fire after positionMaxInterval elapsed — continuous playback would silently lose >30s of position on crash."
        )

        let live = try XCTUnwrap(store.episode(id: ep.id))
        XCTAssertEqual(live.playbackPosition, 3.0, accuracy: 0.001)
    }

    /// Posting `UIApplication.didEnterBackgroundNotification` while a position
    /// update is sitting in the cache must flush it to disk synchronously, so
    /// the user can force-quit + relaunch without losing playback progress.
    func testBackgroundFlushPersistsPendingPosition() async throws {
        let sub = makeSubscription()
        store.upsertPodcast(sub); store.addSubscription(podcastID: sub.id)
        let ep = makeEpisode(podcastID: sub.id, guid: "bg-\(UUID().uuidString)")
        store.upsertEpisodes([ep], forPodcast: sub.id)

        // Eat the eager-first save so the next setEpisodePlaybackPosition
        // call lands in the cache instead of going straight to disk.
        store.setEpisodePlaybackPosition(ep.id, position: 1.0)
        store.persistence.resetSaveInvocationCount()

        // This call should be cached, not flushed.
        store.setEpisodePlaybackPosition(ep.id, position: 42.0)
        XCTAssertEqual(
            store.persistence.saveInvocationCount, 0,
            "Second position update inside the debounce window leaked through to disk."
        )

        NotificationCenter.default.post(
            name: UIApplication.didEnterBackgroundNotification,
            object: nil
        )
        // The observer hops back to the main actor before flushing; let it
        // run before we assert.
        await Task.yield()
        try await Task.sleep(for: .milliseconds(50))

        XCTAssertGreaterThanOrEqual(
            store.persistence.saveInvocationCount, 1,
            "didEnterBackgroundNotification did not trigger an immediate flush."
        )

        let reopened = await AppStateTestSupport.makeIsolatedStore(
            fileURL: storeFileURL,
            reset: false
        )
        XCTAssertEqual(
            reopened.store.episode(id: ep.id)?.playbackPosition ?? -1, 42.0,
            accuracy: 0.001,
            "Background flush did not persist the pending position."
        )
    }

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
