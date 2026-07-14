import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayShows
//
// Builds the "Shows" tab. The root template is a list of followed podcasts,
// sorted alphabetically. Tapping a row pushes a nested `CPListTemplate` with
// that show's recent episodes. The detail-line / show-name pattern matches
// what Apple Podcasts uses in CarPlay so the experience reads as native.

@MainActor
enum CarPlayShows {

    static var itemCap: Int { CPListTemplate.maximumItemCount }

    /// Build the root podcasts list. Selecting a row pushes a per-show
    /// episode list via the `onSelectShow` callback.
    static func makeRootTemplate(
        store: AppStateStore,
        onSelectShow: @escaping (Podcast) -> Void
    ) -> CPListTemplate {
        let podcasts = Array(store.sortedFollowedPodcasts.prefix(itemCap))

        let items = podcasts.map { podcast -> CPListItem in
            let detail = makeShowDetail(podcast: podcast, store: store)
            let item = CPListItem(
                text: podcast.title,
                detailText: detail,
                image: UIImage(systemName: "dot.radiowaves.left.and.right")
            )
            item.accessoryType = .disclosureIndicator
            item.handler = { _, completion in
                onSelectShow(podcast)
                completion()
            }
            CarPlayArtwork.fetch(podcast.imageURL) { [weak item] image in
                guard let item, let image else { return }
                item.setImage(image)
            }
            return item
        }

        let template = CPListTemplate(title: "Shows", sections: [CPListSection(items: items)])
        template.tabImage = UIImage(systemName: "square.grid.2x2.fill")
        template.tabTitle = "Shows"
        template.emptyViewTitleVariants = ["No subscriptions"]
        template.emptyViewSubtitleVariants = [
            "Subscribe to podcasts on your iPhone — they'll appear here."
        ]
        return template
    }

    /// Build the per-show episode list pushed when the driver taps a podcast.
    /// Newest episodes first; played episodes are dimmed via the
    /// `isPlayed` flag.
    static func makeEpisodesTemplate(
        for podcast: Podcast,
        store: AppStateStore,
        onSelect: @escaping (Episode) -> Void
    ) -> CPListTemplate {
        let episodes = Array(store.episodes(forPodcast: podcast.id).prefix(itemCap))
        let items = episodes.map { episode -> CPListItem in
            let item = CPListItem(
                text: episode.title,
                detailText: makeEpisodeDetail(episode),
                image: UIImage(systemName: "waveform")
            )
            item.accessoryType = .disclosureIndicator
            item.playbackProgress = progressFraction(for: episode)
            item.handler = { _, completion in
                onSelect(episode)
                completion()
            }
            let url = episode.imageURL ?? podcast.imageURL
            CarPlayArtwork.fetch(url) { [weak item] image in
                guard let item, let image else { return }
                item.setImage(image)
            }
            return item
        }
        return CPListTemplate(title: podcast.title, sections: [CPListSection(items: items)])
    }

    // MARK: - Formatting

    private static func makeShowDetail(podcast: Podcast, store: AppStateStore) -> String {
        let unplayed = store.unplayedCountByShow[podcast.id] ?? 0
        if unplayed > 0 {
            return "\(unplayed) unplayed"
        }
        if !podcast.author.isEmpty {
            return podcast.author
        }
        return ""
    }

    private static func makeEpisodeDetail(_ episode: Episode) -> String {
        let date = Self.relativeFormatter.localizedString(for: episode.pubDate, relativeTo: Date())
        guard let duration = episode.duration, duration > 0 else { return date }
        if episode.playbackPosition > 0 && !episode.played {
            let remaining = max(0, duration - episode.playbackPosition)
            return "\(date) · \(shortDuration(seconds: remaining)) left"
        }
        return "\(date) · \(shortDuration(seconds: duration))"
    }

    private static func progressFraction(for episode: Episode) -> CGFloat {
        guard let duration = episode.duration, duration > 0 else { return 0 }
        return CGFloat(max(0, min(1, episode.playbackPosition / duration)))
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
