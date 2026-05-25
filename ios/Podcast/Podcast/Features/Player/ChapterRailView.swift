import SwiftUI

// MARK: - ChapterRailView
//
// Horizontal scrollable rail of episode chapter markers. Rendered inside
// `PlayerView` when `episode.chapters` is non-empty.
//
// Doctrine:
//   D7 — the rail emits a `podcast.player.seek` action on tap; Rust decides
//        whether to honor it. The view does no playhead arithmetic beyond
//        the read-only "currently playing" highlight.
//   D5 — empty chapter list means the rail is absent entirely; no skeleton.
//   D2 — pure read of the snapshot (`chapters` + `currentPositionSecs`);
//        no derived caches.

struct ChapterRailView: View {
    let chapters: [ChapterSummary]
    let currentPositionSecs: Double
    let onSeek: (Double) -> Void

    var body: some View {
        if !chapters.isEmpty {
            ScrollViewReader { proxy in
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: PodcastSpace.s) {
                        ForEach(Array(chapters.enumerated()), id: \.offset) { index, chapter in
                            ChapterChip(
                                index: index,
                                chapter: chapter,
                                isCurrent: isCurrent(chapter: chapter, index: index),
                                onTap: { onSeek(chapter.startSecs) }
                            )
                            .id(index)
                        }
                    }
                    .padding(.horizontal, PodcastSpace.l)
                }
                .onChange(of: activeIndex) { _, newIndex in
                    guard let newIndex else { return }
                    withAnimation(.easeInOut(duration: 0.25)) {
                        proxy.scrollTo(newIndex, anchor: .center)
                    }
                }
            }
        }
    }

    // MARK: - Current-chapter logic

    /// Index of the chapter that contains `currentPositionSecs`, or the last
    /// chapter whose `startSecs <= currentPositionSecs` when no `endSecs`
    /// boundary is published. `nil` when the playhead is before the first
    /// chapter (rare — chapter 0 typically starts at 0).
    private var activeIndex: Int? {
        var candidate: Int?
        for (index, chapter) in chapters.enumerated() {
            if chapter.startSecs <= currentPositionSecs {
                if let end = chapter.endSecs, currentPositionSecs >= end {
                    continue
                }
                candidate = index
            } else {
                break
            }
        }
        return candidate
    }

    private func isCurrent(chapter: ChapterSummary, index: Int) -> Bool {
        activeIndex == index
    }
}

// MARK: - Chip

private struct ChapterChip: View {
    let index: Int
    let chapter: ChapterSummary
    let isCurrent: Bool
    let onTap: () -> Void

    var body: some View {
        Button(action: onTap) {
            HStack(spacing: PodcastSpace.xs) {
                Text("\(index + 1)")
                    .font(PodcastFont.caption.weight(.semibold))
                    .foregroundStyle(isCurrent ? Color.black : Color.white.opacity(0.65))
                    .frame(width: 18, height: 18)
                    .background(
                        Circle()
                            .fill(isCurrent ? Color.white : Color.white.opacity(0.18))
                    )
                Text(chapter.title)
                    .font(PodcastFont.callout.weight(isCurrent ? .semibold : .regular))
                    .foregroundStyle(isCurrent ? Color.white : Color.white.opacity(0.75))
                    .lineLimit(1)
                Text(formatTimestamp(chapter.startSecs))
                    .font(PodcastFont.caption.monospacedDigit())
                    .foregroundStyle(Color.white.opacity(0.55))
            }
            .padding(.horizontal, PodcastSpace.m)
            .padding(.vertical, PodcastSpace.s)
            .background(
                Capsule()
                    .fill(isCurrent ? Color.white.opacity(0.18) : Color.white.opacity(0.08))
            )
            .overlay(
                Capsule()
                    .strokeBorder(
                        isCurrent ? Color.white.opacity(0.6) : Color.clear,
                        lineWidth: 1
                    )
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Chapter \(index + 1): \(chapter.title)")
        .accessibilityHint("Seek to \(formatTimestamp(chapter.startSecs))")
    }

    private func formatTimestamp(_ secs: Double) -> String {
        let total = max(0, Int(secs.rounded()))
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 {
            return String(format: "%d:%02d:%02d", h, m, s)
        }
        return String(format: "%d:%02d", m, s)
    }
}
