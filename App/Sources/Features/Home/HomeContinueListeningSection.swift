import SwiftUI

// MARK: - HomeContinueListeningSection

/// Compact "Continue Listening" strip at the top of Home. Shows up to 3
/// in-progress episodes (pubDate within the last 2 weeks) as vertical rows,
/// with a "See All" button when the full list has more. Swipe any row
/// trailing to remove it from the list without marking it played.
struct HomeContinueListeningSection: View {
    let episodes: [Episode]
    let onPlay: (Episode) -> Void
    let onRemove: (Episode) -> Void
    let onSeeAll: () -> Void

    @Environment(AppStateStore.self) private var store

    /// Sum of every visible row's own content height, reported by each row's
    /// `RowHeightReader` background and combined via `RowContentHeightKey`.
    /// Drives `rowList`'s explicit `.frame(height:)` — see that property's
    /// doc comment for why a plain `List` can't size itself here.
    @State private var summedRowContentHeight: CGFloat = 0

    private static let rowVerticalInset = AppTheme.Spacing.sm * 2
    private static let separatorHeight: CGFloat = 0.5

    private var visible: [Episode] {
        Array(episodes.prefix(3))
    }

    private var listHeight: CGFloat {
        let insets = CGFloat(visible.count) * Self.rowVerticalInset
        let separators = CGFloat(max(0, visible.count - 1)) * Self.separatorHeight
        return summedRowContentHeight + insets + separators
    }

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            header
            rowList
        }
    }

    private var header: some View {
        HStack {
            Text("Continue Listening")
                .font(AppTheme.Typography.title3)
                .foregroundStyle(.primary)
            Spacer()
            if episodes.count > 3 {
                Button(action: onSeeAll) {
                    Text("See All")
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(.tint)
                }
                .buttonStyle(.plain)
            }
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.bottom, AppTheme.Spacing.xs)
    }

    /// A real `List` (not a hand-rolled `DragGesture`) so removal uses
    /// genuine `.swipeActions()` — same primitive as `ContinueListeningView`
    /// and `AllPodcastsListView`. Scrolling is disabled because this strip
    /// is embedded inside Home's own outer `ScrollView`, but a `List` given
    /// `.scrollDisabled(true)` does not shrink to fit its content on its
    /// own — it still collapses to zero height unless told an explicit
    /// height. Each row reports its own rendered height via a
    /// `GeometryReader` background (`RowContentHeightKey`); `listHeight`
    /// sums those plus the fixed insets/separators to get the real total,
    /// so the strip grows correctly under larger Dynamic Type instead of
    /// clipping or leaving dead space.
    @ViewBuilder
    private var rowList: some View {
        List {
            ForEach(Array(visible.enumerated()), id: \.element.id) { index, ep in
                ContinueListeningRow(
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID),
                    onPlay: { onPlay(ep) }
                )
                .background(
                    GeometryReader { proxy in
                        Color.clear.preference(key: RowContentHeightKey.self, value: proxy.size.height)
                    }
                )
                .swipeActions(edge: .trailing, allowsFullSwipe: true) {
                    Button(role: .destructive) {
                        Haptics.warning()
                        onRemove(ep)
                    } label: {
                        Label("Remove", systemImage: "xmark.circle")
                    }
                }
                .listRowInsets(EdgeInsets(
                    top: AppTheme.Spacing.sm,
                    leading: AppTheme.Spacing.md,
                    bottom: AppTheme.Spacing.sm,
                    trailing: AppTheme.Spacing.md
                ))
                .listRowBackground(Color(.systemGroupedBackground))
                .listRowSeparatorTint(AppTheme.Tint.hairline)
                .listRowSeparator(index < visible.count - 1 ? .visible : .hidden, edges: .bottom)
            }
        }
        .listStyle(.plain)
        .scrollDisabled(true)
        .scrollContentBackground(.hidden)
        .frame(height: listHeight)
        .onPreferenceChange(RowContentHeightKey.self) { summedRowContentHeight = $0 }
    }
}

/// Sums every row's reported content height (see `rowList`'s doc comment).
private struct RowContentHeightKey: PreferenceKey {
    static let defaultValue: CGFloat = 0
    static func reduce(value: inout CGFloat, nextValue: () -> CGFloat) {
        value += nextValue()
    }
}

// MARK: - ContinueListeningRow

struct ContinueListeningRow: View {
    let episode: Episode
    let podcast: Podcast?
    let onPlay: () -> Void

    var body: some View {
        Button(action: onPlay) {
            HStack(spacing: AppTheme.Spacing.sm) {
                artwork
                meta
                Spacer(minLength: AppTheme.Spacing.sm)
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHint("Resumes this episode")
    }

    // MARK: Subviews

    private var artworkURL: URL? {
        episode.imageURL ?? podcast?.imageURL
    }

    private var artwork: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                .fill(Color(.tertiarySystemFill))
            if let url = artworkURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 88, height: 88)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        Image(systemName: "waveform")
                            .font(.system(size: 16, weight: .light))
                            .foregroundStyle(.secondary)
                    }
                }
            } else {
                Image(systemName: "waveform")
                    .font(.system(size: 16, weight: .light))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
        .overlay(progressArc, alignment: .bottom)
    }

    private var progressArc: some View {
        GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.black.opacity(0.3))
                    .frame(height: 2)
                Capsule()
                    .fill(Color.white)
                    .frame(width: geo.size.width * progressFraction, height: 2)
            }
        }
        .frame(height: 2)
        .padding(.horizontal, 3)
        .padding(.bottom, 3)
    }

    private var meta: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let showName = podcast?.title, !showName.isEmpty {
                Text(showName)
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Text(episode.title)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.primary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
            Text(remainingLabel)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: Helpers

    private var progressFraction: Double {
        guard let duration = episode.duration, duration > 0 else { return 0 }
        return max(0.02, min(1, episode.playbackPosition / duration))
    }

    private var remainingLabel: String {
        guard let duration = episode.duration, duration > 0 else { return "Resume" }
        let remaining = max(0, duration - episode.playbackPosition)
        let total = Int(remaining.rounded())
        let h = total / 3600
        let m = (total % 3600) / 60
        if h > 0 { return "\(h)h \(m)m left" }
        if m > 0 { return "\(m) min left" }
        return "<1 min left"
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if let s = podcast?.title, !s.isEmpty { parts.append(s) }
        parts.append(episode.title)
        parts.append(remainingLabel)
        return parts.joined(separator: ", ")
    }
}
