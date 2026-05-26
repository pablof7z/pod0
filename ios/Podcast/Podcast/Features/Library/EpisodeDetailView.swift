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
    @State private var isTranscriptPresented = false

    private var liveStarred: Bool {
        model.library
            .flatMap { $0.episodes }
            .first { $0.id == episode.id }
            .map(\.starred)
            ?? episode.starred
    }

    /// Re-read the episode from the kernel snapshot so the transcript
    /// toolbar button reflects fresh `transcriptUrl` / `transcriptEntries`
    /// fields without re-opening the screen.
    private var liveEpisode: EpisodeSummary {
        model.library
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

                EpisodeShowNotes(notes: episode.description)
                EpisodeChaptersSection(episode: liveEpisode, podcast: podcast)
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
                    model.dispatch(
                        namespace: "podcast",
                        body: ["op": "star_episode", "episode_id": episode.id]
                    )
                } label: {
                    Image(systemName: liveStarred ? "bookmark.fill" : "bookmark")
                }
                .accessibilityLabel(liveStarred ? "Remove bookmark" : "Bookmark episode")
            }
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
        .sheet(isPresented: $isTranscriptPresented) {
            TranscriptView(episode: liveEpisode, podcast: podcast)
                .environment(model)
        }
    }

    // MARK: - Live snapshot

    /// Re-read the player state so the play/pause label tracks transport.
    private var nowPlaying: PlayerState? { model.nowPlaying }

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
