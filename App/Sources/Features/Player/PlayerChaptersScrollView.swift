import SwiftUI

// MARK: - PlayerChaptersScrollView

/// Chapter rail for the full-screen `PlayerView`.
///
/// Renders a non-scrolling `LazyVStack` of chapter rows interleaved with any
/// episode-anchored notes — both sorted chronologically by their timeline
/// position. The parent owns the `ScrollView` so everything scrolls naturally
/// with the artwork header rather than in a self-contained box.
///
/// Active chapter is highlighted; the parent handles one-time scroll-to-active
/// on open via its own `ScrollViewReader`. Tap to seek; if the player is
/// paused on a fresh open, also start playback. Notes render as lighter
/// annotation rows and can be deleted via long-press context menu.
struct PlayerChaptersScrollView: View {

    let chapters: [Episode.Chapter]
    /// Episode-anchored notes to interleave with chapters. Supplied by
    /// `PlayerView` from `store.notes(forEpisode:)`.
    var notes: [Note] = []
    @Bindable var state: PlaybackState

    /// Live store handle — needed for context-menu note deletion and for the
    /// long-press "Ask agent about this chapter" dispatch.
    @Environment(AppStateStore.self) var store

    /// The chapter that contains the current playhead.
    private var activeChapterID: UUID? {
        chapters.active(at: state.currentTime)?.id
    }

    private var adSegments: [Episode.AdSegment] {
        guard let id = state.episode?.id,
              let episode = store.episode(id: id) else { return [] }
        return episode.adSegments ?? []
    }

    /// Chapters and notes merged and sorted by their timeline position.
    private var railItems: [ChapterRailItem] {
        let chapterItems = chapters.map { ChapterRailItem.chapter($0) }
        let noteItems    = notes.map    { ChapterRailItem.note($0)    }
        return (chapterItems + noteItems).sorted { $0.sortTime < $1.sortTime }
    }

    var body: some View {
        LazyVStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            ForEach(railItems) { item in
                switch item {
                case .chapter(let chapter):
                    chapterRow(chapter, isActive: chapter.id == activeChapterID)
                        .id(chapter.id)
                case .note(let note):
                    noteRow(note)
                        .id(note.id)
                }
            }
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Chapters")
    }

    // MARK: - Chapter row

    @ViewBuilder
    private func chapterRow(_ chapter: Episode.Chapter, isActive: Bool) -> some View {
        let overlapsAd = chapter.overlapsAd(in: chapters, adSegments: adSegments)
        let duration = PlayerChapterPresentation.duration(
            for: chapter,
            in: chapters,
            episodeDuration: state.duration
        )
        let playedFraction = PlayerChapterPresentation.progress(
            for: chapter,
            in: chapters,
            episodeDuration: state.duration,
            currentTime: state.currentTime
        )
        Button {
            handleTap(chapter)
        } label: {
            HStack(alignment: .firstTextBaseline, spacing: AppTheme.Spacing.sm) {
                Text(chapter.title)
                    .font(.system(.body).weight(isActive ? .bold : .regular))
                    .foregroundStyle(isActive ? Color.primary : Color.secondary)
                    .multilineTextAlignment(.leading)
                    .lineLimit(2)
                Spacer(minLength: 0)
                if overlapsAd {
                    Image(systemName: "speaker.slash")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(AppTheme.Tint.warning)
                        .accessibilityLabel("Contains an ad")
                }
                if let duration, let label = PlayerTimeFormat.approximateDuration(duration) {
                    Text(label)
                        .font(.footnote.weight(.medium))
                        .foregroundStyle(Color.secondary)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.sm)
            .padding(.vertical, AppTheme.Spacing.sm)
            .background {
                chapterProgressBackground(
                    fraction: playedFraction,
                    isActive: isActive
                )
            }
            .overlay(alignment: .leading) {
                if overlapsAd {
                    RoundedRectangle(cornerRadius: 1.5, style: .continuous)
                        .fill(AppTheme.Tint.warning)
                        .frame(width: 3)
                        .padding(.vertical, 4)
                        .accessibilityHidden(true)
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(chapter.title)
        .accessibilityValue(
            accessibilityValue(
                duration: duration,
                playedFraction: playedFraction,
                isActive: isActive
            )
        )
        .accessibilityHint("Seeks playback to this chapter")
        .contextMenu {
            Button {
                askAgent(about: chapter)
            } label: {
                Label("Ask agent about this chapter", systemImage: "sparkles")
            }
        }
    }

    private func chapterProgressBackground(fraction: Double, isActive: Bool) -> some View {
        GeometryReader { proxy in
            ZStack(alignment: .leading) {
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(Color.primary.opacity(isActive ? 0.045 : 0.018))
                Rectangle()
                    .fill(Color.accentColor.opacity(isActive ? 0.10 : 0.045))
                    .frame(width: proxy.size.width * fraction.clamped01)
            }
            .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous))
        }
        .accessibilityHidden(true)
    }

    private func accessibilityValue(
        duration: TimeInterval?,
        playedFraction: Double,
        isActive: Bool
    ) -> String {
        var details: [String] = []
        if isActive { details.append("Active chapter") }
        if let duration, let label = PlayerTimeFormat.approximateDuration(duration) {
            details.append("About \(label)")
        }
        if playedFraction >= 1 {
            details.append("Played")
        } else if playedFraction > 0 {
            details.append("\(Int((playedFraction * 100).rounded())) percent played")
        }
        return details.joined(separator: ", ")
    }

    private func askAgent(about chapter: Episode.Chapter) {
        ChapterAskAgentDispatcher.dispatch(
            chapter: chapter,
            in: chapters,
            episode: state.episode,
            store: store
        )
    }

    // MARK: - Behavior

    private func handleTap(_ chapter: Episode.Chapter) {
        let isFreshSession = state.currentTime <= 0.5
        Haptics.selection()
        state.navigationalSeek(to: chapter.startTime)
        if !state.isPlaying && isFreshSession {
            state.play()
        }
    }

}

enum PlayerChapterPresentation {
    static func duration(
        for chapter: Episode.Chapter,
        in chapters: [Episode.Chapter],
        episodeDuration: TimeInterval
    ) -> TimeInterval? {
        resolvedEnd(
            for: chapter,
            in: chapters,
            episodeDuration: episodeDuration
        ).map { $0 - chapter.startTime }
    }

    static func progress(
        for chapter: Episode.Chapter,
        in chapters: [Episode.Chapter],
        episodeDuration: TimeInterval,
        currentTime: TimeInterval
    ) -> Double {
        guard currentTime.isFinite, currentTime > chapter.startTime,
              let end = resolvedEnd(
                for: chapter,
                in: chapters,
                episodeDuration: episodeDuration
              ) else { return 0 }
        return ((currentTime - chapter.startTime) / (end - chapter.startTime)).clamped01
    }

    private static func resolvedEnd(
        for chapter: Episode.Chapter,
        in chapters: [Episode.Chapter],
        episodeDuration: TimeInterval
    ) -> TimeInterval? {
        var candidates: [TimeInterval] = []
        if let explicitEnd = chapter.endTime {
            candidates.append(explicitEnd)
        }
        if let index = chapters.firstIndex(where: { $0.id == chapter.id }) {
            let next = chapters.index(after: index)
            if next < chapters.endIndex {
                candidates.append(chapters[next].startTime)
            }
        }
        candidates.append(episodeDuration)
        return candidates
            .filter { $0.isFinite && $0 > chapter.startTime }
            .min()
    }
}
