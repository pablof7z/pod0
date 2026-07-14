import SwiftUI

// MARK: - HomeContinueListeningSection

/// Compact "Continue Listening" strip at the top of Home. Shows up to 3
/// in-progress episodes (pubDate within the last 2 weeks) as vertical rows,
/// with a "See All" button when the full list has more. Swipe any row left
/// to remove it from the list without marking it played.
struct HomeContinueListeningSection: View {
    let episodes: [Episode]
    let onPlay: (Episode) -> Void
    let onRemove: (Episode) -> Void
    let onSeeAll: () -> Void

    @Environment(AppStateStore.self) private var store

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

    @ViewBuilder
    private var rowList: some View {
        let visible = Array(episodes.prefix(3))
        VStack(spacing: 0) {
            ForEach(Array(visible.enumerated()), id: \.element.id) { index, ep in
                ContinueListeningSwipeRow(
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID),
                    onPlay: { onPlay(ep) },
                    onRemove: { onRemove(ep) }
                )
                if index < visible.count - 1 {
                    Divider()
                        .background(AppTheme.Tint.hairline)
                        .padding(.leading, AppTheme.Spacing.md + 44 + AppTheme.Spacing.sm)
                }
            }
        }
    }
}

// MARK: - ContinueListeningSwipeRow

/// Wraps a `ContinueListeningRow` with a swipe-left gesture that reveals a
/// red Remove button. Removing resets playback position to zero so the
/// episode leaves the "Continue Listening" strip but stays in the library.
private struct ContinueListeningSwipeRow: View {
    let episode: Episode
    let podcast: Podcast?
    let onPlay: () -> Void
    let onRemove: () -> Void

    @State private var isRevealed = false
    @State private var dragExtra: CGFloat = 0

    private let revealWidth: CGFloat = 80

    var body: some View {
        let offset = (isRevealed ? -revealWidth : 0) + dragExtra

        ZStack(alignment: .trailing) {
            Button(role: .destructive) {
                withAnimation(.spring(response: 0.25)) {
                    isRevealed = false
                    dragExtra = 0
                }
                onRemove()
            } label: {
                VStack(spacing: 4) {
                    Image(systemName: "xmark.circle.fill")
                        .font(.title3)
                    Text("Remove")
                        .font(.caption2.weight(.semibold))
                }
                .foregroundStyle(.white)
                .frame(width: revealWidth)
                .frame(maxHeight: .infinity)
                .background(Color.red)
            }
            .buttonStyle(.plain)

            ContinueListeningRow(episode: episode, podcast: podcast, onPlay: onPlay)
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.sm)
                .background(Color(.systemGroupedBackground))
                .offset(x: offset)
                .gesture(
                    DragGesture(minimumDistance: 10)
                        .onChanged { value in
                            guard abs(value.translation.height) < abs(value.translation.width) else { return }
                            let start: CGFloat = isRevealed ? -revealWidth : 0
                            let proposed = start + value.translation.width
                            dragExtra = max(-revealWidth, min(0, proposed)) - start
                        }
                        .onEnded { value in
                            let start: CGFloat = isRevealed ? -revealWidth : 0
                            let final = start + value.translation.width
                            withAnimation(.spring(response: 0.3, dampingFraction: 0.8)) {
                                dragExtra = 0
                                isRevealed = final < -(revealWidth * 0.5)
                            }
                        }
                )
        }
        .clipped()
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
