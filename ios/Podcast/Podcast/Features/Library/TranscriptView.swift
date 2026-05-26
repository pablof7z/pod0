import SwiftUI

// MARK: - TranscriptView
//
// Full-screen sheet that renders the structured transcript for one episode.
// The transcript is owned by the Rust kernel: when the user opens the sheet
// (or taps "Load Transcript") we dispatch `podcast.fetch_transcript`; the
// parsed rows land on the next snapshot tick as
// `EpisodeSummary.transcriptEntries`.
//
// Doctrine:
//   D2 — no business logic in the shell. The kernel decides whether a
//        transcript exists, how to fetch it, and how to parse it. The shell
//        only renders + seeks.
//   D5 — no default-zero renders. When the kernel has no entries we show an
//        explicit empty / load state.

struct TranscriptView: View {
    let episode: EpisodeSummary
    let podcast: PodcastSummary

    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// `true` after the user opens the sheet so we don't re-trigger the
    /// auto-fetch on every snapshot tick (the kernel is idempotent but
    /// avoiding the dispatch keeps the log clean).
    @State private var didAutoFetch = false

    // MARK: - Live snapshot

    /// Re-read the episode from `model.library` on every tick so freshly
    /// fetched transcript entries appear without re-opening the sheet. Falls
    /// back to the caller-supplied row when the episode disappears from the
    /// library (e.g. the user unsubscribed mid-sheet).
    private var liveEpisode: EpisodeSummary {
        model.library
            .flatMap { $0.episodes }
            .first { $0.id == episode.id }
            ?? episode
    }

    private var entries: [TranscriptEntry] {
        liveEpisode.transcriptEntries ?? []
    }

    private var nowPlaying: PlayerState? {
        model.nowPlaying
    }

    private var isThisEpisodePlaying: Bool {
        nowPlaying?.episodeId == episode.id
    }

    /// Index of the active entry (if any) keyed off the live player position.
    /// Returns `nil` when no entry has `startSecs <= position` (i.e. we're
    /// before the first row), so the view doesn't paint a phantom highlight.
    private var activeIndex: Int? {
        guard isThisEpisodePlaying, let position = nowPlaying?.positionSecs else { return nil }
        return Self.activeIndex(for: position, in: entries)
    }

    // MARK: - Body

    var body: some View {
        NavigationStack {
            Group {
                if !entries.isEmpty {
                    transcriptList
                } else if liveEpisode.transcriptUrl != nil {
                    loadPrompt
                } else {
                    unavailable
                }
            }
            .navigationTitle("Transcript")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
        }
        .task {
            // Kick the kernel once when the sheet opens. Fire-and-forget —
            // the parsed entries arrive on the next snapshot tick.
            guard !didAutoFetch, liveEpisode.transcriptUrl != nil, entries.isEmpty else { return }
            didAutoFetch = true
            dispatchFetch()
        }
    }

    // MARK: - States

    @ViewBuilder
    private var transcriptList: some View {
        ScrollViewReader { proxy in
            List {
                header
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: AppTheme.Spacing.md,
                                               leading: AppTheme.Spacing.lg,
                                               bottom: AppTheme.Spacing.md,
                                               trailing: AppTheme.Spacing.lg))

                ForEach(Array(entries.enumerated()), id: \.offset) { idx, entry in
                    TranscriptRowView(
                        entry: entry,
                        isActive: idx == activeIndex,
                        onTap: { seek(to: entry.startSecs) }
                    )
                    .id(idx)
                    .listRowSeparator(.hidden)
                    .listRowInsets(EdgeInsets(top: AppTheme.Spacing.xs,
                                               leading: AppTheme.Spacing.lg,
                                               bottom: AppTheme.Spacing.xs,
                                               trailing: AppTheme.Spacing.lg))
                }
            }
            .listStyle(.plain)
            .onChange(of: activeIndex) { _, newIndex in
                // Only follow playback while the episode is actively playing —
                // otherwise we'd fight the user when they're scrolling to read
                // ahead during pause.
                guard let newIndex,
                      nowPlaying?.isPlaying == true else { return }
                withAnimation(.easeInOut(duration: 0.25)) {
                    proxy.scrollTo(newIndex, anchor: .center)
                }
            }
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text(liveEpisode.title)
                .font(AppTheme.Typography.headline)
                .multilineTextAlignment(.leading)

            Text(podcast.title)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private var loadPrompt: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Spacer(minLength: 0)
            Image(systemName: "text.bubble")
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(.secondary)
            VStack(spacing: AppTheme.Spacing.xs) {
                Text("Transcript Available")
                    .font(AppTheme.Typography.headline)
                Text("Tap to fetch the publisher transcript for this episode.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
                    .padding(.horizontal, AppTheme.Spacing.lg)
            }
            Button {
                Haptics.medium()
                didAutoFetch = true
                dispatchFetch()
            } label: {
                Text("Load Transcript")
                    .font(AppTheme.Typography.headline)
                    .padding(.vertical, AppTheme.Spacing.sm)
                    .padding(.horizontal, AppTheme.Spacing.xl)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color.accentColor)
                    )
                    .foregroundStyle(Color.white)
            }
            .buttonStyle(.plain)
            Spacer(minLength: 0)
        }
        .padding(AppTheme.Spacing.lg)
        .frame(maxWidth: .infinity, maxHeight: .infinity)
    }

    private var unavailable: some View {
        ContentUnavailableView(
            "No Transcript",
            systemImage: "text.bubble",
            description: Text("This episode does not advertise a publisher transcript.")
        )
    }

    // MARK: - Dispatch

    private func dispatchFetch() {
        model.dispatch(
            namespace: "podcast",
            body: ["op": "fetch_transcript", "episode_id": episode.id]
        )
    }

    private func seek(to positionSecs: Double) {
        Haptics.light()
        model.dispatch(
            namespace: "podcast.player",
            body: ["op": "seek", "position_secs": positionSecs]
        )
    }

    // MARK: - Active-index helper

    /// Find the entry whose `[startSecs, endSecs)` interval contains
    /// `position`. When `endSecs` is `nil` we fall back to the largest
    /// `startSecs <= position` — that matches the "untimed" fallback the
    /// Rust projection emits for plain-text transcripts.
    static func activeIndex(for position: Double, in entries: [TranscriptEntry]) -> Int? {
        guard !entries.isEmpty else { return nil }
        // Exact-interval pass first.
        for (idx, entry) in entries.enumerated() {
            if let end = entry.endSecs, position >= entry.startSecs && position < end {
                return idx
            }
        }
        // Fallback: largest startSecs <= position. Entries are produced
        // newest-by-time-order by the parsers, so a linear scan is correct.
        var best: Int? = nil
        for (idx, entry) in entries.enumerated() {
            if entry.startSecs <= position {
                best = idx
            } else {
                break
            }
        }
        return best
    }
}
