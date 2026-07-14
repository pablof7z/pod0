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
                Text(formatTimestamp(chapter.startTime))
                    .font(.system(.footnote, design: .monospaced).weight(.medium))
                    .foregroundStyle(Color.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.sm)
            .padding(.vertical, AppTheme.Spacing.sm)
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
        .accessibilityValue(isActive ? "Active chapter, \(formatTimestamp(chapter.startTime))" : formatTimestamp(chapter.startTime))
        .accessibilityHint("Seeks playback to this chapter")
        .contextMenu {
            Button {
                askAgent(about: chapter)
            } label: {
                Label("Ask agent about this chapter", systemImage: "sparkles")
            }
        }
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

    private func formatTimestamp(_ t: TimeInterval) -> String {
        guard t.isFinite, t >= 0 else { return "0:00" }
        let total = Int(t)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        return h > 0
            ? String(format: "%02d:%02d:%02d", h, m, s)
            : String(format: "%02d:%02d", m, s)
    }
}
