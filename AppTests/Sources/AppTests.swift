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

    // MARK: - Models

    func testAnchorCodable() throws {
        let anchor = Anchor.note(id: UUID())
        let data = try JSONEncoder().encode(anchor)
        let decoded = try JSONDecoder().decode(Anchor.self, from: data)
        XCTAssertEqual(anchor, decoded)
    }

    // MARK: - AgentPrompt

    func testAgentPromptIncludesMemories() {
        var state = AppState()
        state.agentMemories.append(AgentMemory(content: "User prefers mornings"))

        let prompt = AgentPrompt.build(for: state)

        XCTAssertTrue(prompt.contains("User prefers mornings"))
    }

    func testAgentPromptIncludesSubscriptions() {
        var state = AppState()
        let p1 = makeSubscription(title: "The Tim Ferriss Show")
        let p2 = makeSubscription(title: "Acquired")
        state.podcasts.append(contentsOf: [p1, p2])
        state.subscriptions.append(contentsOf: [
            PodcastSubscription(podcastID: p1.id),
            PodcastSubscription(podcastID: p2.id),
        ])

        let prompt = AgentPrompt.build(for: state)

        XCTAssertTrue(prompt.contains("## Subscriptions (2)"))
        XCTAssertTrue(prompt.contains("The Tim Ferriss Show"))
        XCTAssertTrue(prompt.contains("Acquired"))
    }

    func testAgentPromptIncludesInProgressEpisodes() {
        var state = AppState()
        let sub = makeSubscription(title: "Lex Fridman")
        state.podcasts.append(sub)
        state.subscriptions.append(PodcastSubscription(podcastID: sub.id))
        var ep = makeEpisode(podcastID: sub.id, guid: "ip-1")
        ep.title = "Episode about something"
        ep.playbackPosition = 600
        state.episodes.append(ep)

        let prompt = AgentPrompt.build(for: state)

        XCTAssertTrue(prompt.contains("## In Progress"))
        XCTAssertTrue(prompt.contains("Episode about something"))
        XCTAssertTrue(prompt.contains("Lex Fridman"))
    }

    func testAgentPromptIncludesRecentUnplayedEpisodes() {
        var state = AppState()
        let sub = makeSubscription(title: "Recent Show")
        state.podcasts.append(sub)
        state.subscriptions.append(PodcastSubscription(podcastID: sub.id))
        var fresh = makeEpisode(podcastID: sub.id, guid: "fresh-1")
        fresh.title = "Brand new episode"
        fresh.pubDate = Date().addingTimeInterval(-3600)
        state.episodes.append(fresh)

        let prompt = AgentPrompt.build(for: state)

        XCTAssertTrue(prompt.contains("## Recent"))
        XCTAssertTrue(prompt.contains("Brand new episode"))
    }

    func testAgentPromptOmitsOldEpisodesFromRecentSection() {
        var state = AppState()
        let sub = makeSubscription(title: "Old Show")
        state.podcasts.append(sub)
        state.subscriptions.append(PodcastSubscription(podcastID: sub.id))
        var old = makeEpisode(podcastID: sub.id, guid: "old-1")
        old.title = "Old episode title that is unique"
        old.pubDate = Date().addingTimeInterval(-30 * 86_400)
        state.episodes.append(old)

        let prompt = AgentPrompt.build(for: state)

        // Subscription should still appear, but the 30-day-old episode
        // shouldn't surface in the 7-day recent window.
        XCTAssertFalse(prompt.contains("Old episode title that is unique"))
    }

    // MARK: - Persistence isolation

    /// Regression test for the test-leak bug: writing through an isolated
    /// store must NOT mutate the production App Group state file.
    func testIsolatedStoreDoesNotTouchSharedAppGroupContainer() throws {
        let productionURL = Persistence.appGroupStateFileURL
        // Snapshot whatever the production file currently holds (may be
        // absent on a clean dev machine — `nil` is a valid baseline).
        let before = try? Data(contentsOf: productionURL)

        // Make a noisy unmigrated-domain mutation through the isolated store.
        _ = store.addAgentMemory(content: "Leak Canary \(UUID().uuidString)")

        // The production file must be byte-identical to the snapshot.
        let after = try? Data(contentsOf: productionURL)
        XCTAssertEqual(before, after, "Test mutation leaked into the shared App Group state file.")
    }

    // MARK: - Settings

    func testSettingsDoesNotPersistLegacyOpenRouterAPIKey() throws {
        let json = """
        {
          "llmModel": "openai/gpt-4o-mini",
          "openRouterAPIKey": "sk-or-v1-secret",
          "agentMaxTurns": 12
        }
        """.data(using: .utf8)!

        let decoded = try JSONDecoder().decode(Settings.self, from: json)
        XCTAssertEqual(decoded.openRouterCredentialSource, .manual)
        XCTAssertEqual(decoded.legacyOpenRouterAPIKey, "sk-or-v1-secret")

        let encoded = try JSONEncoder().encode(decoded)
        let encodedString = String(data: encoded, encoding: .utf8) ?? ""
        XCTAssertFalse(encodedString.contains("sk-or-v1-secret"))
        XCTAssertFalse(encodedString.contains("openRouterAPIKey"))
    }

    func testSettingsPersistsBYOKMetadataOnly() throws {
        var settings = Settings()
        settings.markOpenRouterBYOK(keyID: "key_123", keyLabel: "Default")

        let encoded = try JSONEncoder().encode(settings)
        let encodedString = String(data: encoded, encoding: .utf8) ?? ""

        XCTAssertTrue(encodedString.contains("byok"))
        XCTAssertTrue(encodedString.contains("key_123"))
        XCTAssertTrue(encodedString.contains("Default"))
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
