import SwiftUI

// MARK: - EpisodeChaptersSection

/// Chapters surface inside `EpisodeDetailView`. Manages its own chapter-compilation
/// state so the parent view stays free of chapter-specific `@State` vars.
///
/// Renders either:
///   - A tappable row listing the chapter count (opens `ChaptersView`)
///   - A "Generate chapters" button (when a raw transcript exists but no chapters)
///   - Nothing (no transcript, no chapters)
struct EpisodeChaptersSection: View {
    let episode: EpisodeSummary
    let podcast: PodcastSummary

    @Environment(KernelModel.self) private var model
    @State private var isCompilingChapters: Bool = false
    @State private var showChaptersSheet: Bool = false

    var body: some View {
        let chapters = liveChapters
        if !chapters.isEmpty {
            chaptersAvailableRow(count: chapters.count, hasAI: chapters.contains(where: \.isAiGenerated))
        } else if hasRawTranscript {
            generateChaptersButton
        }
    }

    // MARK: - Live snapshot

    private var liveChapters: [ChapterSummary] {
        let library = model.library
        guard let show = library.first(where: { $0.id == podcast.id }),
              let ep = show.episodes.first(where: { $0.id == episode.id }) else {
            return episode.chapters ?? []
        }
        return ep.chapters ?? []
    }

    private var hasRawTranscript: Bool {
        let library = model.library
        guard let show = library.first(where: { $0.id == podcast.id }),
              let ep = show.episodes.first(where: { $0.id == episode.id }) else {
            return (episode.transcript ?? "").isEmpty == false
        }
        return (ep.transcript ?? "").isEmpty == false
    }

    // MARK: - Chapter row

    private func chaptersAvailableRow(count: Int, hasAI: Bool) -> some View {
        Button {
            Haptics.light()
            showChaptersSheet = true
        } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "list.bullet.rectangle")
                    .font(.system(size: 16, weight: .semibold))
                Text("\(count) chapter\(count == 1 ? "" : "s")")
                    .font(AppTheme.Typography.headline)
                if hasAI {
                    Image(systemName: "sparkles")
                        .font(.system(size: 14, weight: .semibold))
                        .foregroundStyle(.purple)
                        .accessibilityLabel("AI generated")
                }
                Spacer()
                Image(systemName: "chevron.right")
                    .font(.system(size: 14, weight: .semibold))
                    .foregroundStyle(.tertiary)
            }
            .padding(.vertical, AppTheme.Spacing.md)
            .padding(.horizontal, AppTheme.Spacing.md)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(Color.secondary.opacity(0.12))
            )
            .foregroundStyle(.primary)
        }
        .buttonStyle(.plain)
        .sheet(isPresented: $showChaptersSheet) {
            ChaptersView(episodeId: episode.id, podcastId: podcast.id)
                .environment(model)
        }
    }

    // MARK: - Generate button

    @ViewBuilder
    private var generateChaptersButton: some View {
        Button {
            Haptics.medium()
            isCompilingChapters = true
            model.dispatch(
                namespace: "podcast.chapters",
                body: ["op": "compile", "episode_id": episode.id]
            )
        } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                if isCompilingChapters {
                    ProgressView()
                        .controlSize(.small)
                        .tint(.purple)
                } else {
                    Image(systemName: "sparkles")
                        .font(.system(size: 16, weight: .semibold))
                }
                Text(isCompilingChapters ? "Generating chapters…" : "Generate chapters")
                    .font(AppTheme.Typography.headline)
                Spacer()
            }
            .padding(.vertical, AppTheme.Spacing.md)
            .padding(.horizontal, AppTheme.Spacing.md)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .stroke(Color.purple.opacity(0.55), lineWidth: 1)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color.purple.opacity(0.08))
                    )
            )
            .foregroundStyle(.primary)
        }
        .buttonStyle(.plain)
        .disabled(isCompilingChapters)
        .accessibilityLabel(isCompilingChapters ? "Generating chapters" : "Generate chapters from transcript")
        .onChange(of: liveChapters.isEmpty) { _, isEmpty in
            if !isEmpty { isCompilingChapters = false }
        }
        .task(id: isCompilingChapters) {
            guard isCompilingChapters else { return }
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            if isCompilingChapters { isCompilingChapters = false }
        }
    }
}
