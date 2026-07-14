import SwiftUI
import WidgetKit

// MARK: - NowPlayingWidget

/// Home-screen widget surfacing the currently-loaded podcast episode.
/// Small variant shows artwork + title; medium adds the show name, a
/// progress bar, and the remaining time.
///
/// Tap deep-links into the app. The system-wide widget gesture model only
/// supports a `widgetURL` per family, so the URL is the same in every
/// size: `podcastr://` (cold-launch into the foreground tab the user
/// last selected). When nothing is loaded the widget renders an empty
/// state inviting the user back into the app.
struct NowPlayingWidget: Widget {
    static let kind = "io.f7z.podcast.now-playing"

    var body: some WidgetConfiguration {
        StaticConfiguration(kind: Self.kind, provider: NowPlayingTimelineProvider()) { entry in
            NowPlayingWidgetView(entry: entry)
                .containerBackground(.fill.tertiary, for: .widget)
        }
        .configurationDisplayName("Now Playing")
        .description("See what's playing right now.")
        .supportedFamilies([.systemSmall, .systemMedium])
    }
}

// MARK: - NowPlayingWidgetView

/// Family-aware root view for the widget. Branches on `widgetFamily` so the
/// small layout (artwork-dominant) and medium layout (artwork + metadata
/// column) live in dedicated builders without per-family conditionals
/// scattered across the body.
struct NowPlayingWidgetView: View {
    @Environment(\.widgetFamily) private var family
    let entry: NowPlayingEntry

    var body: some View {
        Group {
            if let snapshot = entry.snapshot, !snapshot.episodeTitle.isEmpty {
                switch family {
                case .systemMedium: NowPlayingMediumView(snapshot: snapshot)
                default:            NowPlayingSmallView(snapshot: snapshot)
                }
            } else {
                NowPlayingEmptyView()
            }
        }
        .widgetURL(URL(string: "podcastr://"))
    }
}

// MARK: - Small variant

private struct NowPlayingSmallView: View {
    let snapshot: NowPlayingSnapshot

    var body: some View {
        VStack(alignment: .leading, spacing: 8) {
            NowPlayingArtwork(urlString: snapshot.imageURLString, size: 56)
                .clipShape(RoundedRectangle(cornerRadius: 10, style: .continuous))
            Spacer(minLength: 0)
            Text(snapshot.episodeTitle)
                .font(WidgetTheme.Typography.smallSubtitle)
                .foregroundStyle(.primary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }
}

// MARK: - Medium variant

private struct NowPlayingMediumView: View {
    let snapshot: NowPlayingSnapshot

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            NowPlayingArtwork(urlString: snapshot.imageURLString, size: 76)
                .clipShape(RoundedRectangle(cornerRadius: 12, style: .continuous))

            VStack(alignment: .leading, spacing: 4) {
                Text(snapshot.episodeTitle)
                    .font(WidgetTheme.Typography.itemTitle.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                // Chapter title — preferred over the show name when
                // present because it's the more specific "where am I right
                // now" signal once playback is in flight. Falls back to
                // the show name for chapter-less episodes.
                if let chapter = snapshot.chapterTitle, !chapter.isEmpty {
                    Label(chapter, systemImage: "book.pages")
                        .labelStyle(WidgetChapterLabelStyle())
                        .font(WidgetTheme.Typography.accessoryRow)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                } else if !snapshot.showName.isEmpty {
                    Text(snapshot.showName)
                        .font(WidgetTheme.Typography.accessoryRow)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Spacer(minLength: 0)
                progressFooter
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity, alignment: .topLeading)
    }

    @ViewBuilder
    private var progressFooter: some View {
        let total = max(snapshot.duration, 0)
        let position = max(0, min(snapshot.position, total))
        let fraction: Double = total > 0 ? position / total : 0
        VStack(alignment: .leading, spacing: 4) {
            ProgressView(value: fraction)
                .progressViewStyle(.linear)
                .tint(WidgetTheme.Colors.brandIndigo)
            HStack(spacing: 4) {
                // Tiny play/pause indicator — without this the widget
                // shows a progress bar that looks like it's advancing
                // even when the user has paused, since timeline refresh
                // and on-disk position both update on a delay.
                Image(systemName: (snapshot.isPlaying ?? false) ? "play.fill" : "pause.fill")
                    .font(.system(size: 9, weight: .semibold))
                    .foregroundStyle(.secondary)
                Spacer()
                Text(remainingLabel(position: position, duration: total))
                    .font(WidgetTheme.Typography.accessoryRow)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
        }
    }

    private func remainingLabel(position: TimeInterval, duration: TimeInterval) -> String {
        guard duration > 0 else { return "" }
        let remaining = max(0, duration - position)
        return "−" + Self.formatter.string(from: remaining).orEmpty
    }

    private static let formatter: DateComponentsFormatter = {
        let f = DateComponentsFormatter()
        f.allowedUnits = [.hour, .minute, .second]
        f.unitsStyle = .positional
        f.zeroFormattingBehavior = [.pad]
        return f
    }()
}

// MARK: - Empty state

private struct NowPlayingEmptyView: View {
    var body: some View {
        VStack(spacing: 8) {
            Image(systemName: "headphones")
                .font(WidgetTheme.Typography.emptyIcon)
                .foregroundStyle(WidgetTheme.Colors.brandGradient)
            Text("Tap to open Podcastr")
                .font(WidgetTheme.Typography.emptyTitle)
                .multilineTextAlignment(.center)
                .foregroundStyle(.primary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }
}

// MARK: - Artwork

/// Async-loaded artwork. The widget process can fetch URLs but should keep
/// the resolved image small — `frame(width:height:)` here caps the
/// downloaded thumbnail's render size so we don't blow widget memory on a
/// 3000×3000 publisher cover. Falls back to a brand glyph on failure.
private struct NowPlayingArtwork: View {
    let urlString: String?
    let size: CGFloat

    var body: some View {
        Group {
            if let urlString, let url = URL(string: urlString) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    case .failure, .empty:
                        placeholder
                    @unknown default:
                        placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: size, height: size)
    }

    private var placeholder: some View {
        ZStack {
            WidgetTheme.Colors.brandGradient
            Image(systemName: "waveform")
                .font(.system(size: size * 0.45, weight: .semibold))
                .foregroundStyle(.white)
        }
    }
}

// MARK: - String helper

private extension Optional where Wrapped == String {
    var orEmpty: String { self ?? "" }
}

// MARK: - Chapter label style

/// Compact horizontal label so the icon + title fit cleanly inside the
/// medium widget's secondary line. Default `Label` styling stacks the icon
/// at a larger weight than we want here.
private struct WidgetChapterLabelStyle: LabelStyle {
    func makeBody(configuration: Configuration) -> some View {
        HStack(spacing: 4) {
            configuration.icon
                .font(.system(size: 9, weight: .semibold))
            configuration.title
        }
    }
}
