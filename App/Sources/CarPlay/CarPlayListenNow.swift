import CarPlay
import Foundation
import UIKit

// MARK: - CarPlayListenNow
//
// Builds the "Listen Now" tab: a two-section list with the user's in-progress
// episodes up top (resume row with elapsed/remaining as the detail line), then
// the latest unplayed episodes across all subscriptions. Mirrors what Apple
// Podcasts calls "Up Next" + "New Releases" in CarPlay.
//
// Builder returns a fully-configured `CPListTemplate`. The scene delegate is
// responsible for assigning it to the tab. Image fetches are launched after
// items are created; CarPlay refreshes the row in place when `setImage(_:)`
// is called, so we don't have to re-push the template.

@MainActor
enum CarPlayListenNow {

    /// Maximum number of items in a single section. CarPlay caps this on a
    /// per-head-unit basis — `CPListTemplate.maximumItemCount` returns the
    /// floor we're guaranteed to be allowed. Older cars cap at ~12; newer
    /// ones at ~100. Slicing here keeps the template push from failing.
    static var sectionCap: Int { CPListTemplate.maximumItemCount }

    static func makeTemplate(
        store: AppStateStore,
        onSelect: @escaping (Episode) -> Void
    ) -> CPListTemplate {
        let inProgress = Array(store.inProgressEpisodes.prefix(sectionCap))
        let recent = Array(store.recentEpisodes(limit: sectionCap).prefix(sectionCap))

        var sections: [CPListSection] = []

        if !inProgress.isEmpty {
            let items = inProgress.map { episode in
                makeItem(for: episode, store: store, showResume: true, onSelect: onSelect)
            }
            sections.append(CPListSection(items: items, header: "Continue listening", sectionIndexTitle: nil))
        }

        if !recent.isEmpty {
            let items = recent.map { episode in
                makeItem(for: episode, store: store, showResume: false, onSelect: onSelect)
            }
            sections.append(CPListSection(items: items, header: "Latest episodes", sectionIndexTitle: nil))
        }

        let template = CPListTemplate(title: "Listen Now", sections: sections)
        template.tabImage = UIImage(systemName: "play.circle.fill")
        template.tabTitle = "Listen Now"
        template.emptyViewTitleVariants = ["Nothing to play yet"]
        template.emptyViewSubtitleVariants = [
            "Subscribe to a podcast on your iPhone and new episodes will show up here."
        ]
        return template
    }

    // MARK: - Item construction

    private static func makeItem(
        for episode: Episode,
        store: AppStateStore,
        showResume: Bool,
        onSelect: @escaping (Episode) -> Void
    ) -> CPListItem {
        let detail = makeDetailLine(for: episode, store: store, showResume: showResume)
        let item = CPListItem(
            text: episode.title,
            detailText: detail,
            image: placeholderArtwork()
        )
        item.accessoryType = .disclosureIndicator
        item.playbackProgress = playbackProgress(for: episode)
        item.handler = { _, completion in
            onSelect(episode)
            completion()
        }
        let artURL = episode.imageURL ?? store.podcast(id: episode.podcastID)?.imageURL
        CarPlayArtwork.fetch(artURL) { [weak item] image in
            guard let item, let image else { return }
            item.setImage(image)
        }
        return item
    }

    /// Detail line is "Show name · 23 min left" when there's a resume hint,
    /// otherwise "Show name · published date". We bias toward the resume
    /// hint on the Continue Listening section so the driver sees at a glance
    /// how much road they have left in the episode.
    private static func makeDetailLine(
        for episode: Episode,
        store: AppStateStore,
        showResume: Bool
    ) -> String {
        let show = store.podcast(id: episode.podcastID)?.title ?? ""
        if showResume, let duration = episode.duration, duration > 0 {
            let remaining = max(0, duration - episode.playbackPosition)
            let formatted = Self.shortDuration(seconds: remaining)
            if show.isEmpty {
                return "\(formatted) left"
            }
            return "\(show) · \(formatted) left"
        }
        let formatted = Self.relativeDate(episode.pubDate)
        if show.isEmpty {
            return formatted
        }
        return "\(show) · \(formatted)"
    }

    /// 0.0–1.0 progress fraction for the row's playback indicator.
    private static func playbackProgress(for episode: Episode) -> CGFloat {
        guard let duration = episode.duration, duration > 0 else { return 0 }
        let fraction = episode.playbackPosition / duration
        return CGFloat(max(0, min(1, fraction)))
    }

    private static func placeholderArtwork() -> UIImage? {
        UIImage(systemName: "waveform")
    }

    // MARK: - Formatting

    private static let relativeFormatter: RelativeDateTimeFormatter = {
        let f = RelativeDateTimeFormatter()
        f.unitsStyle = .short
        return f
    }()

    private static func relativeDate(_ date: Date) -> String {
        relativeFormatter.localizedString(for: date, relativeTo: Date())
    }

    private static func shortDuration(seconds: TimeInterval) -> String {
        let total = Int(seconds.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 { return "\(h)h \(m)m" }
        return "\(max(1, m)) min"
    }
}
