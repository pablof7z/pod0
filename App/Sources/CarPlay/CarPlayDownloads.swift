import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayDownloads
//
// Builds the "Downloads" tab — episodes whose enclosure is already on disk.
// This is the tab that matters most on a road trip: if cell coverage drops,
// the driver can still play whatever's been downloaded without a stutter.
//
// Sorted newest-first across all shows. The detail line carries the show
// name so the driver can identify the episode at a glance without reading
// the longer episode title.

@MainActor
enum CarPlayDownloads {

    static var itemCap: Int { CPListTemplate.maximumItemCount }

    static func makeTemplate(
        store: AppStateStore,
        onSelect: @escaping (Episode) -> Void
    ) -> CPListTemplate {
        let downloaded = store.state.episodes
            .filter {
                if case .downloaded = $0.downloadState { return true }
                return false
            }
            .sorted { $0.pubDate > $1.pubDate }
            .prefix(itemCap)

        let items = downloaded.map { episode -> CPListItem in
            let item = CPListItem(
                text: episode.title,
                detailText: makeDetail(episode, store: store),
                image: UIImage(systemName: "arrow.down.circle.fill")
            )
            item.accessoryType = .disclosureIndicator
            item.playbackProgress = progressFraction(for: episode)
            item.handler = { _, completion in
                onSelect(episode)
                completion()
            }
            let url = episode.imageURL ?? store.podcast(id: episode.podcastID)?.imageURL
            CarPlayArtwork.fetch(url) { [weak item] image in
                guard let item, let image else { return }
                item.setImage(image)
            }
            return item
        }

        let template = CPListTemplate(title: "Downloads", sections: [CPListSection(items: Array(items))])
        template.tabImage = UIImage(systemName: "arrow.down.circle.fill")
        template.tabTitle = "Downloads"
        template.emptyViewTitleVariants = ["No downloads"]
        template.emptyViewSubtitleVariants = [
            "Download episodes on your iPhone before you drive — they'll be ready to play here even when you lose signal."
        ]
        return template
    }

    private static func makeDetail(_ episode: Episode, store: AppStateStore) -> String {
        let show = store.podcast(id: episode.podcastID)?.title ?? ""
        guard let duration = episode.duration, duration > 0 else { return show }
        if episode.playbackPosition > 0 && !episode.played {
            let remaining = max(0, duration - episode.playbackPosition)
            let left = shortDuration(seconds: remaining)
            return show.isEmpty ? "\(left) left" : "\(show) · \(left) left"
        }
        let full = shortDuration(seconds: duration)
        return show.isEmpty ? full : "\(show) · \(full)"
    }

    private static func progressFraction(for episode: Episode) -> CGFloat {
        guard let duration = episode.duration, duration > 0 else { return 0 }
        return CGFloat(max(0, min(1, episode.playbackPosition / duration)))
    }

    private static func shortDuration(seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 { return "\(h)h \(m)m" }
        return "\(max(1, m)) min"
    }
}
