import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayLibraryTemplates
//
// Pure builder for the CarPlay Library tab.
//
//   * `makeLibraryTemplate(library:onSelectShow:)` — root template. One
//     row per subscribed podcast (newest-first sort by latest episode
//     timestamp). Tap pushes the per-show episode list.
//   * `makeEpisodesTemplate(podcast:onSelectEpisode:)` — per-show list.
//     Episodes are embedded inside `PodcastSummary.episodes` (the
//     kernel projection ships them together), so we don't need a second
//     snapshot read.
//
// CarPlay caps `CPListTemplate` item counts per head unit
// (`CPListTemplate.maximumItemCount` returns the floor). Slicing here
// keeps the push from failing on older cars.
//
// D6: artwork loading is intentionally absent — the ios/Podcast target
// does not link Kingfisher, and synchronous `Data(contentsOf:)` would
// block the head-unit transport. We render with a placeholder system
// glyph; the system-owned `CPNowPlayingTemplate` continues to receive
// artwork through `MPNowPlayingInfoCenter` (configured in
// `AudioCapability+NowPlaying`).

@MainActor
enum CarPlayLibraryTemplates {

    private static var itemCap: Int { CPListTemplate.maximumItemCount }

    // MARK: - Root template

    static func makeLibraryTemplate(
        library: [PodcastSummary],
        onSelectShow: @escaping (PodcastSummary) -> Void
    ) -> CPListTemplate {
        let sorted = library.sorted { lhs, rhs in
            latestEpisodeTimestamp(lhs) > latestEpisodeTimestamp(rhs)
        }
        let podcasts = Array(sorted.prefix(itemCap))

        let items = podcasts.map { podcast -> CPListItem in
            let item = CPListItem(
                text: podcast.title,
                detailText: makeShowDetail(podcast: podcast),
                image: UIImage(systemName: "dot.radiowaves.left.and.right")
            )
            item.accessoryType = .disclosureIndicator
            item.handler = { _, completion in
                onSelectShow(podcast)
                completion()
            }
            return item
        }

        let template = CPListTemplate(
            title: "Library",
            sections: [CPListSection(items: items)])
        template.tabImage = UIImage(systemName: "books.vertical")
        template.tabTitle = "Library"
        template.emptyViewTitleVariants = ["No subscriptions yet"]
        template.emptyViewSubtitleVariants = [
            "Subscribe to podcasts on your iPhone — they'll appear here."
        ]
        return template
    }

    // MARK: - Per-show episodes

    static func makeEpisodesTemplate(
        podcast: PodcastSummary,
        onSelectEpisode: @escaping (EpisodeSummary) -> Void
    ) -> CPListTemplate {
        let sorted = podcast.episodes.sorted { lhs, rhs in
            (lhs.publishedAt ?? 0) > (rhs.publishedAt ?? 0)
        }
        let episodes = Array(sorted.prefix(itemCap))

        let items = episodes.map { episode -> CPListItem in
            let item = CPListItem(
                text: episode.title,
                detailText: makeEpisodeDetail(episode),
                image: UIImage(systemName: "waveform")
            )
            item.accessoryType = .disclosureIndicator
            item.handler = { _, completion in
                onSelectEpisode(episode)
                completion()
            }
            return item
        }

        let template = CPListTemplate(
            title: podcast.title,
            sections: [CPListSection(items: items)])
        template.emptyViewTitleVariants = ["No episodes yet"]
        template.emptyViewSubtitleVariants = [
            "Episodes will appear here as soon as the feed refreshes."
        ]
        return template
    }

    // MARK: - Formatting helpers

    private static func latestEpisodeTimestamp(_ podcast: PodcastSummary) -> Int {
        podcast.episodes.compactMap { $0.publishedAt }.max() ?? 0
    }

    private static func makeShowDetail(podcast: PodcastSummary) -> String {
        if podcast.unplayedCount > 0 {
            return "\(podcast.unplayedCount) unplayed"
        }
        if let author = podcast.author, !author.isEmpty {
            return author
        }
        if podcast.episodeCount > 0 {
            return "\(podcast.episodeCount) episodes"
        }
        return ""
    }

    private static func makeEpisodeDetail(_ episode: EpisodeSummary) -> String {
        var parts: [String] = []
        if let ts = episode.publishedAt {
            parts.append(relativeDate(from: ts))
        }
        if let dur = episode.durationSecs, dur > 0 {
            parts.append(shortDuration(seconds: dur))
        }
        return parts.joined(separator: " · ")
    }

    private static func relativeDate(from unixSecs: Int) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unixSecs))
        return relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    private static func shortDuration(seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 { return "\(h)h \(m)m" }
        return "\(max(1, m)) min"
    }
}
