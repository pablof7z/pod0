import XCTest
@testable import Podcastr

final class PodcastSearchTests: XCTestCase {
    func testLocalSearchFindsShowsAndEpisodes() {
        let podcastID = UUID()
        var state = AppState()
        state.podcasts = [
            Podcast(
                id: podcastID,
                feedURL: URL(string: "https://example.com/feed.xml")!,
                title: "Nutrition Lab",
                author: "Dr. Rivera",
                description: "Keto metabolism and insulin conversations.",
                categories: ["Health"]
            )
        ]
        state.subscriptions = [PodcastSubscription(podcastID: podcastID)]
        state.episodes = [
            Episode(
                podcastID: podcastID,
                guid: "keto-1",
                title: "Keto and insulin sensitivity",
                description: "A discussion of appetite and glucose.",
                pubDate: Date(timeIntervalSince1970: 1),
                enclosureURL: URL(string: "https://example.com/e.mp3")!
            )
        ]

        let results = PodcastSearchEngine.localResults(query: "keto insulin", state: state)

        XCTAssertEqual(results.episodes.first?.episode.guid, "keto-1")
        XCTAssertEqual(results.shows.first?.podcast.title, "Nutrition Lab")
    }

    func testWikiSearchFindsClaimBodies() {
        let page = WikiPage(
            slug: "metabolic-health",
            title: "Metabolic Health",
            kind: .topic,
            scope: .global,
            summary: "A page about long-term health.",
            sections: [
                WikiSection(
                    heading: "Claims",
                    kind: .freeform,
                    ordinal: 0,
                    claims: [
                        WikiClaim(text: "Keto changes appetite regulation in the discussed episode.")
                    ]
                )
            ]
        )

        let results = PodcastSearchEngine.wikiResults(query: "keto appetite", pages: [page])

        XCTAssertEqual(results.first?.page.slug, "metabolic-health")
        XCTAssertTrue(results.first?.excerpt.contains("Keto") == true)
    }
}
