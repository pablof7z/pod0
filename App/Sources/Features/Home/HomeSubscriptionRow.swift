import SwiftUI

// MARK: - HomeSubscriptionRow

/// Compact list row for the merged Home subscription list. `AppTheme.Layout.iconLg`
/// artwork on the leading edge, show title + most-recent-episode preview in the
/// middle, and a recency pill on the trailing edge. Long-press surfaces
/// the same context menu (Refresh / Unsubscribe) the old grid used so the
/// existing muscle memory carries over.
struct HomeSubscriptionRow: View {
    let podcast: Podcast
    let mostRecentEpisode: Episode?
    let now: Date
    /// `false` for podcasts the app knows about but the user does not
    /// follow. Drives the destructive action's wording — there is no
    /// subscription to leave, so the menu offers "Delete" instead.
    var isFollowed: Bool = true
    /// Fired when the user picks the destructive action from the context
    /// menu. The parent owns the confirmation alert (so the destructive
    /// flow lives next to the rest of the list state, not inside the row).
    let onRequestRemove: () -> Void

    @Environment(\.dynamicTypeSize) private var dynamicTypeSize

    var body: some View {
        NavigationLink(value: podcast) {
            HStack(alignment: .center, spacing: AppTheme.Spacing.md) {
                artwork
                meta
                if !dynamicTypeSize.isAccessibilitySize {
                    Spacer(minLength: AppTheme.Spacing.sm)
                    recencyPill
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            SubscriptionContextMenu(
                podcast: podcast,
                isFollowed: isFollowed,
                onRequestRemove: onRequestRemove
            )
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    // MARK: - Subviews

    @ViewBuilder
    private var artwork: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                .fill(
                    LinearGradient(
                        colors: [
                            podcast.accentColor.opacity(0.95),
                            podcast.accentColor.opacity(0.55)
                        ],
                        startPoint: .topLeading,
                        endPoint: .bottomTrailing
                    )
                )
            if let url = podcast.imageURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: AppTheme.Layout.iconLg * 2, height: AppTheme.Layout.iconLg * 2)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        Image(systemName: podcast.artworkSymbol)
                            .font(.system(size: 21, weight: .light))
                            .foregroundStyle(.white.opacity(0.92))
                    }
                }
            } else {
                Image(systemName: podcast.artworkSymbol)
                    .font(.system(size: 21, weight: .light))
                    .foregroundStyle(.white.opacity(0.92))
            }
        }
        .frame(width: AppTheme.Layout.iconLg, height: AppTheme.Layout.iconLg)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text(podcast.title)
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
            if let preview = mostRecentEpisodePreview {
                Text(preview)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(dynamicTypeSize.isAccessibilitySize ? 2 : 1)
            }
            if dynamicTypeSize.isAccessibilitySize {
                recencyPill
            }
        }
    }

    private var recencyPill: some View {
        Text(recencyLabel)
            .font(AppTheme.Typography.caption2)
            .monospacedDigit()
            .foregroundStyle(.secondary)
            .padding(.horizontal, AppTheme.Spacing.xs)
            .padding(.vertical, 2)
    }

    // MARK: - Helpers

    /// Stripped, single-line preview of the most-recent-episode title.
    /// Falls back to a "no episodes yet" hint when the show has no
    /// episodes — the row still renders so the user can tap through to
    /// the show detail and pull-to-refresh there.
    private var mostRecentEpisodePreview: String? {
        guard let ep = mostRecentEpisode else {
            return "No episodes yet"
        }
        let title = ep.title.trimmingCharacters(in: .whitespacesAndNewlines)
        return title.isEmpty ? nil : title
    }

    /// Recency label using the existing `RelativeTimestamp.extended`
    /// formatter — same "2h ago" / "3w ago" / abbreviated date strings
    /// used elsewhere so the surface stays visually consistent.
    private var recencyLabel: String {
        guard let date = mostRecentEpisode?.pubDate else {
            return "—"
        }
        return RelativeTimestamp.extended(date, now: now)
    }

    private var accessibilityLabel: String {
        var parts: [String] = [podcast.title]
        if let preview = mostRecentEpisodePreview { parts.append(preview) }
        parts.append("Last episode \(recencyLabel)")
        return parts.joined(separator: ", ")
    }
}
