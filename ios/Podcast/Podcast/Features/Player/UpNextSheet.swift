import SwiftUI

// MARK: - UpNextSheet
//
// Modal sheet listing the kernel-owned playback queue ("Up Next").
//
// Doctrine:
//   D7 — all mutations are Rust-side. The sheet only dispatches
//        `podcast.player.dequeue`, `podcast.player.clear_queue`, and
//        `podcast.player.play_next`; it never mutates a local copy of
//        the queue. The list re-renders from the next snapshot tick.
//   D5 — the queue projection lives on `PodcastUpdate.queue` so it
//        stays visible even when `nowPlaying` is `nil`.
//
// Adding to the queue is handled elsewhere (the show-detail row's
// swipe action / context menu dispatches `enqueue`). This sheet only
// renders + removes.

struct UpNextSheet: View {
    @Environment(KernelModel.self) private var model
    @Environment(\.dismiss) private var dismiss

    /// Resolved queue rows: the snapshot stores episode ids, so we
    /// look up the matching `EpisodeSummary` + parent `PodcastSummary`
    /// from `model.library`. Episodes that have since been unsubscribed
    /// (and thus aren't in the library anymore) are dropped silently —
    /// the kernel still holds the id but the UI has nothing to render.
    private var rows: [QueueRow] {
        let ids = model.podcastSnapshot?.queue ?? []
        guard !ids.isEmpty else { return [] }
        var index: [String: (EpisodeSummary, PodcastSummary)] = [:]
        for podcast in model.library {
            for ep in podcast.episodes {
                index[ep.id] = (ep, podcast)
            }
        }
        return ids.compactMap { id in
            guard let pair = index[id] else { return nil }
            return QueueRow(episode: pair.0, podcast: pair.1)
        }
    }

    var body: some View {
        NavigationStack {
            content
                .navigationTitle("Up Next")
                .navigationBarTitleDisplayMode(.inline)
                .toolbar {
                    ToolbarItem(placement: .topBarLeading) {
                        Button("Done") { dismiss() }
                    }
                    ToolbarItem(placement: .topBarTrailing) {
                        if !rows.isEmpty {
                            Button(role: .destructive) {
                                Haptics.warning()
                                model.dispatch(
                                    namespace: "podcast.player",
                                    body: ["op": "clear_queue"]
                                )
                            } label: {
                                Text("Clear")
                            }
                        }
                    }
                }
        }
    }

    @ViewBuilder
    private var content: some View {
        if rows.isEmpty {
            emptyState
        } else {
            queueList
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView {
            Label("Your queue is empty", systemImage: "text.line.first.and.arrowtriangle.forward")
        } description: {
            Text("Add episodes to Up Next from any show to line them up.")
        }
    }

    // MARK: - List

    private var queueList: some View {
        List {
            Section {
                Button {
                    Haptics.medium()
                    model.dispatch(
                        namespace: "podcast.player",
                        body: ["op": "play_next"]
                    )
                    NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
                    dismiss()
                } label: {
                    HStack(spacing: AppTheme.Spacing.sm) {
                        Image(systemName: "play.fill")
                        Text("Play Next")
                            .font(AppTheme.Typography.headline)
                        Spacer()
                    }
                    .padding(.vertical, AppTheme.Spacing.xs)
                    .foregroundStyle(Color.accentColor)
                }
                .buttonStyle(.plain)
                .accessibilityHint("Plays the first episode in your queue.")
            }

            Section("Queue") {
                ForEach(rows) { row in
                    UpNextRow(row: row) {
                        Haptics.selection()
                        model.dispatch(
                            namespace: "podcast.player",
                            body: ["op": "dequeue", "episode_id": row.id]
                        )
                    }
                }
            }
        }
        .listStyle(.insetGrouped)
    }
}

// MARK: - QueueRow model

/// Resolved row data — pre-joined `EpisodeSummary` + parent
/// `PodcastSummary` so the row view doesn't re-scan the library.
private struct QueueRow: Identifiable {
    let episode: EpisodeSummary
    let podcast: PodcastSummary
    var id: String { episode.id }
}

// MARK: - UpNextRow

/// Single row in the Up Next list. Renders artwork + title + show,
/// with a trailing remove button that dispatches `dequeue`.
private struct UpNextRow: View {
    let row: QueueRow
    let onRemove: () -> Void

    private static let thumbnailSize: CGFloat = 44

    var body: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            thumbnail
            VStack(alignment: .leading, spacing: 2) {
                Text(row.episode.title)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                Text(row.podcast.title)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Spacer(minLength: AppTheme.Spacing.sm)
            Button(role: .destructive) {
                onRemove()
            } label: {
                Image(systemName: "minus.circle.fill")
                    .font(.title3)
                    .foregroundStyle(.secondary)
                    .accessibilityLabel("Remove \(row.episode.title) from queue")
            }
            .buttonStyle(.plain)
        }
        .padding(.vertical, AppTheme.Spacing.xs)
        .swipeActions(edge: .trailing, allowsFullSwipe: true) {
            Button(role: .destructive, action: onRemove) {
                Label("Remove", systemImage: "trash")
            }
        }
    }

    @ViewBuilder
    private var thumbnail: some View {
        let shape = RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
        let urlString = row.episode.artworkUrl ?? row.podcast.artworkUrl
        Group {
            if let s = urlString, let url = URL(string: s) {
                AsyncImage(url: url) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default: placeholder
                    }
                }
            } else {
                placeholder
            }
        }
        .frame(width: Self.thumbnailSize, height: Self.thumbnailSize)
        .clipShape(shape)
        .accessibilityHidden(true)
    }

    private var placeholder: some View {
        ZStack {
            Color.secondary.opacity(0.18)
            Image(systemName: "waveform")
                .font(.system(size: 16, weight: .light))
                .foregroundStyle(.secondary)
        }
    }
}
