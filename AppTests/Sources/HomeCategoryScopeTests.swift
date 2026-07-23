import XCTest
@testable import Podcastr

final class HomeCategoryScopeTests: XCTestCase {
    func testEpisodesInCategoryNilFilterReturnsInputUntouched() {
        let firstPodcastID = UUID()
        let secondPodcastID = UUID()
        let episodes = [
            makeEpisode(podcastID: firstPodcastID),
            makeEpisode(podcastID: secondPodcastID),
        ]

        let scoped = HomeCategoryScope.episodesInCategory(
            episodes,
            allowedSubscriptionIDs: nil
        )

        XCTAssertEqual(scoped.map(\.podcastID), [firstPodcastID, secondPodcastID])
    }

    func testEpisodesInCategoryFiltersToAllowedSubscriptions() {
        let firstPodcastID = UUID()
        let secondPodcastID = UUID()
        let thirdPodcastID = UUID()

        let scoped = HomeCategoryScope.episodesInCategory(
            [
                makeEpisode(podcastID: firstPodcastID),
                makeEpisode(podcastID: secondPodcastID),
                makeEpisode(podcastID: thirdPodcastID),
            ],
            allowedSubscriptionIDs: [firstPodcastID, thirdPodcastID]
        )

        XCTAssertEqual(Set(scoped.map(\.podcastID)), [firstPodcastID, thirdPodcastID])
    }

    func testEpisodesInCategoryEmptyAllowedSetReturnsEmpty() {
        let episodes = [makeEpisode(podcastID: UUID())]

        let scoped = HomeCategoryScope.episodesInCategory(
            episodes,
            allowedSubscriptionIDs: []
        )

        XCTAssertTrue(scoped.isEmpty)
    }

    private func makeEpisode(podcastID: UUID) -> Episode {
        Episode(
            podcastID: podcastID,
            guid: UUID().uuidString,
            title: "Episode",
            pubDate: Date(),
            enclosureURL: URL(string: "https://example.com/episode.mp3")!
        )
    }
}
