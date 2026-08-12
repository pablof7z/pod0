import XCTest
@testable import Podcastr

@MainActor
final class PodcastCatalogEpisodeSearchServiceTests: XCTestCase {
    func testFuzzyEpisodeAndPodcastHintsRankTheIntendedEpisodeFirst() throws {
        let feedURL = try XCTUnwrap(URL(string: "https://example.com/hidden-brain.xml"))
        let podcastID = UUID()
        let podcast = Podcast(
            id: podcastID,
            feedURL: feedURL,
            title: "Hidden Brain",
            author: "Shankar Vedantam"
        )
        let intended = episode(
            podcastID: podcastID,
            title: "Tiny Changes, Remarkable Results",
            description: "James Clear explains how to build better habits."
        )
        let unrelated = episode(
            podcastID: podcastID,
            title: "The Psychology of Money",
            description: "Why spending decisions feel emotional."
        )

        let matches = PodcastCatalogEpisodeSearchService.rank(
            feeds: [.init(podcast: podcast, episodes: [unrelated, intended])],
            episodeQuery: "the one about building good habits with James",
            podcastHint: "hidden brain podcast",
            limit: 2
        )

        XCTAssertEqual(matches.first?.episode.id, intended.id)
    }

    func testRankingReturnsMultiplePlausibleMatchesWithoutExactTitles() throws {
        let feedURL = try XCTUnwrap(URL(string: "https://example.com/acquired.xml"))
        let podcastID = UUID()
        let podcast = Podcast(id: podcastID, feedURL: feedURL, title: "Acquired")
        let episodes = [
            episode(podcastID: podcastID, title: "The Nvidia Story", description: "Jensen Huang and computer chips"),
            episode(podcastID: podcastID, title: "TSMC", description: "The semiconductor foundry behind modern chips"),
            episode(podcastID: podcastID, title: "Hermes", description: "A luxury fashion house"),
        ]

        let matches = PodcastCatalogEpisodeSearchService.rank(
            feeds: [.init(podcast: podcast, episodes: episodes)],
            episodeQuery: "their episodes on computer chips",
            podcastHint: "acquired",
            limit: 2
        )

        XCTAssertEqual(matches.first?.episode.title, "TSMC")
        XCTAssertTrue(matches.contains { $0.episode.title == "The Nvidia Story" })
    }

    private func episode(
        podcastID: UUID,
        title: String,
        description: String
    ) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: UUID().uuidString,
            title: title,
            description: description,
            pubDate: Date(),
            enclosureURL: URL(string: "https://cdn.example.com/\(UUID().uuidString).mp3")!
        )
    }
}
