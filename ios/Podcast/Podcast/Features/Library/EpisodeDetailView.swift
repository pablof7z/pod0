import SwiftUI

// MARK: - EpisodeRoute

/// Navigation value pushed onto a `NavigationStack` to open `EpisodeDetailView`.
///
/// We carry the surrounding `PodcastSummary` along with the `EpisodeSummary`
/// so the detail view can render fallback artwork and the show title without
/// re-querying the kernel snapshot for the parent podcast.
struct EpisodeRoute: Hashable {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
}

// MARK: - EpisodeDetailView

/// NMP-native episode detail screen. Backed entirely by `EpisodeSummary` from
/// the kernel snapshot — no `AppStateStore`, no compat types.
struct EpisodeDetailView: View {
    let episode: EpisodeSummary
    let podcast: PodcastSummary

    @Environment(KernelModel.self) private var model
    @State private var isCommentsSheetPresented: Bool = false

    /// `true` between the moment the user taps "Generate Chapters" and the
    /// first snapshot tick that surfaces non-empty `chapters` for this
    /// episode. The Rust handler is synchronous (returns immediately after
    /// persisting), but iOS needs the next snapshot poll to see the result;
    /// this flag drives the in-flight progress indicator across that gap.
    @State private var isCompilingChapters: Bool = false

    /// Controls presentation of the `ChaptersView` sheet.
    @State private var showChaptersSheet: Bool = false
    @State private var isTranscriptPresented = false

    /// Re-read the episode from the kernel snapshot so the transcript
    /// toolbar button reflects fresh `transcriptUrl` / `transcriptEntries`
    /// fields without re-opening the screen.
    private var liveEpisode: EpisodeSummary {
        model.podcastSnapshot?.library
            .flatMap { $0.episodes }
            .first { $0.id == episode.id }
            ?? episode
    }

    /// Transcript availability: either entries have already been fetched, or
    /// the publisher advertises a transcript URL (so the viewer can dispatch
    /// `fetch_transcript`). Hide the toolbar item entirely when neither
    /// holds — there's nothing the user can do.
    private var hasTranscript: Bool {
        if let entries = liveEpisode.transcriptEntries, !entries.isEmpty { return true }
        return liveEpisode.transcriptUrl != nil
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
                artwork
                    .frame(maxWidth: .infinity)

                VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                    Text(episode.title)
                        .font(AppTheme.Typography.title)
                        .multilineTextAlignment(.leading)

                    Text(podcast.title)
                        .font(AppTheme.Typography.headline)
                        .foregroundStyle(.secondary)

                    if let resumeSecs = episode.playbackPositionSecs {
                        Text("Resume at \(formatDuration(resumeSecs))")
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.tertiary)
                            .accessibilityLabel("Resume playback at \(formatDuration(resumeSecs))")
                    }
                }

                metaRow

                playButton

                showNotes
                chaptersSection
            }
            .padding(.horizontal, AppTheme.Spacing.lg)
            .padding(.vertical, AppTheme.Spacing.lg)
        }
        .background(Color(.systemBackground))
        .navigationTitle("Episode")
        .navigationBarTitleDisplayMode(.inline)
        .toolbar {
            ToolbarItem(placement: .topBarTrailing) {
                Button {
                    Haptics.light()
                    isCommentsSheetPresented = true
                } label: {
                    Image(systemName: "bubble.left.and.text.bubble.right")
                }
                .accessibilityLabel("Comments")
            }
            if hasTranscript {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        Haptics.light()
                        isTranscriptPresented = true
                    } label: {
                        Image(systemName: "text.bubble")
                    }
                    .accessibilityLabel("Transcript")
                }
            }
        }
        .sheet(isPresented: $isCommentsSheetPresented) {
            EpisodeCommentsSheet(
                episodeId: episode.id,
                onDismiss: { isCommentsSheetPresented = false }
            )
        }
        .sheet(isPresented: $showChaptersSheet) {
            ChaptersView(episodeId: episode.id, podcastId: podcast.id)
                .environment(model)
        }
        .onChange(of: liveChapters.isEmpty) { _, isEmpty in
            if !isEmpty { isCompilingChapters = false }
        }
        .task(id: isCompilingChapters) {
            guard isCompilingChapters else { return }
            try? await Task.sleep(nanoseconds: 3_000_000_000)
            if isCompilingChapters { isCompilingChapters = false }
        }
        .sheet(isPresented: $isTranscriptPresented) {
            TranscriptView(episode: liveEpisode, podcast: podcast)
                .environment(model)
        }
    }

    // MARK: - Live snapshot

    /// Re-read the player state so the play/pause label tracks transport.
    private var nowPlaying: PlayerState? { model.podcastSnapshot?.nowPlaying }

    private var isThisEpisodePlaying: Bool {
        nowPlaying?.episodeId == episode.id && nowPlaying?.isPlaying == true
    }

    // MARK: - Artwork

    private var artworkURL: URL? {
        if let s = episode.artworkUrl, let url = URL(string: s) { return url }
        if let s = podcast.artworkUrl, let url = URL(string: s) { return url }
        return nil
    }

    @ViewBuilder
    private var artwork: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
        Group {
            if let url = artworkURL {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: artworkPlaceholder
                    }
                }
            } else {
                artworkPlaceholder
            }
        }
        .aspectRatio(1, contentMode: .fit)
        .frame(maxWidth: 320)
        .clipShape(shape)
        .shadow(color: .black.opacity(0.18), radius: 12, x: 0, y: 6)
        .accessibilityHidden(true)
    }

    private var artworkPlaceholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Meta

    @ViewBuilder
    private var metaRow: some View {
        let hasDuration = episode.durationSecs != nil
        let hasDate = episode.publishedAt != nil
        if hasDuration || hasDate {
            HStack(spacing: AppTheme.Spacing.sm) {
                if let secs = episode.durationSecs {
                    Label(formatDuration(secs), systemImage: "clock")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                if hasDuration && hasDate {
                    Text("·")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tertiary)
                }
                if let ts = episode.publishedAt {
                    Label(absoluteDate(from: ts), systemImage: "calendar")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                Spacer()
            }
            .labelStyle(.titleAndIcon)
        }
    }

    // MARK: - Play button

    private var playButton: some View {
        Button {
            Haptics.medium()
            if isThisEpisodePlaying {
                model.dispatch(namespace: "podcast.player", body: ["op": "pause"])
            } else {
                model.dispatch(
                    namespace: "podcast.player",
                    body: ["op": "play", "episode_id": episode.id]
                )
                NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
            }
        } label: {
            HStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: isThisEpisodePlaying ? "pause.fill" : "play.fill")
                    .font(.system(size: 18, weight: .semibold))
                Text(playButtonLabel)
                    .font(AppTheme.Typography.headline)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, AppTheme.Spacing.md)
            .background(
                RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                    .fill(Color.accentColor)
            )
            .foregroundStyle(Color.white)
        }
        .buttonStyle(.plain)
        .accessibilityLabel(isThisEpisodePlaying ? "Pause" : "\(playButtonLabel) \(episode.title)")
    }

    /// Play / Pause / Resume label that respects the snapshot's stored
    /// resume point. Mirrors the legacy `EpisodeDetailHeroView` behaviour:
    /// shows "Resume" when there is a persisted playhead and the episode
    /// isn't currently playing.
    private var playButtonLabel: String {
        if isThisEpisodePlaying { return "Pause" }
        return episode.playbackPositionSecs != nil ? "Resume" : "Play episode"
    }

    // MARK: - Show notes

    /// Renders `episode.description` when present. The Rust projection
    /// drops empty strings to `None`, so a non-nil value here always
    /// has content. System font only per AGENTS.md typography rules.
    @ViewBuilder
    private var showNotes: some View {
        if let notes = episode.description, !notes.isEmpty {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
                Text("Show notes")
                    .font(AppTheme.Typography.headline)
                    .foregroundStyle(.primary)

                Text(notes)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.leading)
                    .fixedSize(horizontal: false, vertical: true)
                    .textSelection(.enabled)
            }
            .frame(maxWidth: .infinity, alignment: .leading)
        }
    }

    // MARK: - Chapters section

    /// Live chapter list resolved from the snapshot (not the cached
    /// `episode` parameter) so a `podcast.chapters.compile` dispatch that
    /// lands new chapters mid-presentation flips the UI without the caller
    /// re-pushing the navigation route.
    private var liveChapters: [ChapterSummary] {
        guard let library = model.podcastSnapshot?.library,
              let show = library.first(where: { $0.id == podcast.id }),
              let ep = show.episodes.first(where: { $0.id == episode.id }) else {
            return episode.chapters ?? []
        }
        return ep.chapters ?? []
    }

    /// Whether a raw transcript is available for chapter generation.
    private var hasRawTranscript: Bool {
        guard let library = model.podcastSnapshot?.library,
              let show = library.first(where: { $0.id == podcast.id }),
              let ep = show.episodes.first(where: { $0.id == episode.id }) else {
            return (episode.transcript ?? "").isEmpty == false
        }
        return (ep.transcript ?? "").isEmpty == false
    }

    @ViewBuilder
    private var chaptersSection: some View {
        let chapters = liveChapters
        if !chapters.isEmpty {
            chaptersAvailableRow(count: chapters.count, hasAI: chapters.contains(where: \.isAiGenerated))
        } else if hasRawTranscript {
            generateChaptersButton
        }
        // No transcript + no chapters: render nothing. iOS surfaces the
        // "fetch transcript" CTA elsewhere; chapters are downstream of that.
    }

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
    }

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
    }

    // MARK: - Formatting

    private func formatDuration(_ secs: Double) -> String {
        let total = Int(secs)
        let h = total / 3600
        let m = (total % 3600) / 60
        let s = total % 60
        if h > 0 { return String(format: "%d:%02d:%02d", h, m, s) }
        return String(format: "%d:%02d", m, s)
    }

    private func absoluteDate(from unixSeconds: Int) -> String {
        let date = Date(timeIntervalSince1970: TimeInterval(unixSeconds))
        return Self.dateFormatter.string(from: date)
    }

    private static let dateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f
    }()
}
