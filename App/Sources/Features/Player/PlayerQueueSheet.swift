import SwiftUI

// MARK: - PlayerQueueSheet

/// "Up Next" sheet presented from the player's Queue chip.
///
/// Renders a SwiftUI `List` over `state.queue` (`[QueueItem]`), resolves each
/// entry to a live `Episode` via the store, and supports tap-to-play,
/// drag-to-reorder, and swipe-to-delete. Bounded segment items show their
/// time range in the row. Footer summarises total runtime and exposes a
/// destructive "Clear queue" action.
///
/// The queue model lives on `PlaybackState`; this view is purely
/// presentational. Adding/removing episodes elsewhere (e.g. from a list row's
/// context menu) goes through `PlaybackState.enqueue(_:)` directly.
struct PlayerQueueSheet: View {

    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss
    @Bindable var state: PlaybackState

    /// Drives the destructive confirmation when the user taps "Clear queue" —
    /// the queue can represent ten minutes of tap-curation, and an accidental
    /// flick on the footer button shouldn't wipe it without a beat to confirm.
    @State private var confirmClear: Bool = false

    var body: some View {
        NavigationStack {
            Group {
                if isEmpty {
                    emptyState
                } else {
                    queueList
                }
            }
            .navigationTitle("Up Next")
            .navigationBarTitleDisplayMode(.inline)
            .onAppear {
                pruneStaleQueue()
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") { dismiss() }
                }
            }
            // `.alert` rather than `.confirmationDialog` — iOS 26 elides
            // the Cancel button on dialogs anchored close to a tappable
            // element (the trash icon in the queue footer here). See
            // `ShowDetailView`, `StorageSettingsView`, and
            // `EpisodeDetailActionsMenu` for the same trap.
            .alert(
                "Clear the queue?",
                isPresented: $confirmClear
            ) {
                Button("Cancel", role: .cancel) {}
                Button("Clear queue", role: .destructive) {
                    Haptics.warning()
                    state.clearQueue()
                }
            } message: {
                let n = resolvedItems.count
                Text("All \(n) queued item\(n == 1 ? "" : "s") will be removed. This cannot be undone.")
            }
        }
        .presentationDetents([.medium, .large])
        .presentationDragIndicator(.visible)
    }

    // MARK: - Live resolution

    /// Pairs each `QueueItem` with its resolved `Episode`. Drops entries whose
    /// episode no longer exists in the store (e.g. user unsubscribed mid-play).
    private var resolvedItems: [(item: QueueItem, episode: Episode)] {
        state.queue.compactMap { item in
            guard let ep = store.episode(id: item.episodeID) else { return nil }
            return (item, ep)
        }
    }

    private var totalRuntime: TimeInterval {
        resolvedItems.reduce(0) { acc, pair in
            if let start = pair.item.startSeconds, let end = pair.item.endSeconds {
                return acc + max(0, end - start)
            }
            return acc + (pair.episode.duration ?? 0)
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView(
            "Nothing queued",
            systemImage: "list.bullet.rectangle",
            description: Text("Episodes you queue from the library will appear here.")
        )
    }

    private var isEmpty: Bool { resolvedItems.isEmpty }

    // MARK: - List

    private var queueList: some View {
        List {
            Section {
                ForEach(resolvedItems, id: \.item.id) { pair in
                    // Wrapped in a `Button` (not `.onTapGesture`) because
                    // SwiftUI's always-active edit mode can swallow tap
                    // gestures on the trailing edge where the move handle
                    // sits. `Button` reliably hits the whole row.
                    Button {
                        play(item: pair.item, episode: pair.episode)
                    } label: {
                        PlayerQueueRow(
                            episode: pair.episode,
                            showName: store.podcast(id: pair.episode.podcastID)?.title ?? "",
                            showImageURL: store.podcast(id: pair.episode.podcastID)?.imageURL,
                            segmentLabel: pair.item.label,
                            segmentRange: segmentRange(for: pair.item)
                        )
                        .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                    .listRowBackground(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                            .fill(Color.primary.opacity(0.04))
                    )
                    .swipeActions(edge: .trailing, allowsFullSwipe: false) {
                        Button(role: .destructive) {
                            state.removeFromQueue(itemID: pair.item.id)
                            Haptics.light()
                        } label: {
                            Label("Remove", systemImage: "minus.circle")
                        }
                    }
                }
                .onMove { indices, destination in
                    state.moveQueue(from: indices, to: destination) { store.episode(id: $0) }
                    Haptics.selection()
                }
            } footer: {
                footer
            }
        }
        .listStyle(.insetGrouped)
        .environment(\.editMode, .constant(.active))
    }

    private func segmentRange(for item: QueueItem) -> String? {
        guard let start = item.startSeconds, let end = item.endSeconds else { return nil }
        return "\(PlayerTimeFormat.clock(start)) – \(PlayerTimeFormat.clock(end))"
    }

    // MARK: - Footer

    private var footer: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            Text(runtimeSummary)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)

            // Two-step confirm — the actual clear lives in the
            // `.confirmationDialog` attached to the `NavigationStack`. Tapping
            // here just arms the dialog; that protects against the user flicking
            // the footer accidentally while reordering.
            // `role: .destructive` already tints the label red via the
            // semantic system color — overriding with raw `.red` broke
            // the Increase Contrast accessibility tint and bypassed the
            // tinting for older renderers.
            Button(role: .destructive) {
                Haptics.selection()
                confirmClear = true
            } label: {
                Label("Clear queue", systemImage: "trash")
                    .font(AppTheme.Typography.subheadline)
            }
            .buttonStyle(.plain)
        }
        .padding(.top, AppTheme.Spacing.sm)
    }

    private var runtimeSummary: String {
        let count = resolvedItems.count
        let runtime = PlayerTimeFormat.clock(totalRuntime)
        let plural = count == 1 ? "item" : "items"
        return "\(count) \(plural) • \(runtime)"
    }

    // MARK: - Actions

    private func play(item: QueueItem, episode: Episode) {
        Haptics.medium()
        state.removeFromQueue(itemID: item.id)
        state.currentSegmentEndTime = item.endSeconds
        state.setEpisode(episode)
        if let start = item.startSeconds {
            state.engine.seek(to: start)
        }
        state.play()
        dismiss()
    }

    private func pruneStaleQueue() {
        state.pruneQueue { store.episode(id: $0) }
    }
}

// MARK: - PlayerQueueRow

/// Row used inside `PlayerQueueSheet`. Pulled out so the parent stays under
/// the soft line limit and the row gets its own preview surface as the
/// design lane iterates on it.
struct PlayerQueueRow: View {

    let episode: Episode
    let showName: String
    /// Show-level cover art used as a fallback when the episode has no
    /// per-episode `<itunes:image>`. Most feeds only ship show-level
    /// artwork; without this fallback the row would render a generic
    /// waveform glyph for those episodes.
    var showImageURL: URL? = nil
    /// Optional label for agent-curated segment items (e.g. chapter title).
    var segmentLabel: String? = nil
    /// Formatted time range string for bounded segments, e.g. "2:30 – 5:45".
    var segmentRange: String? = nil

    var body: some View {
        HStack(spacing: AppTheme.Spacing.md) {
            artwork

            VStack(alignment: .leading, spacing: 2) {
                if !showName.isEmpty {
                    Text(showName)
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(episode.title)
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.primary)
                    .lineLimit(2)
                if let label = segmentLabel {
                    Text(label)
                        .font(.caption2.italic())
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                if let range = segmentRange {
                    Text(range)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                } else if let runtime = formattedRuntime {
                    Text(runtime)
                        .font(.caption2)
                        .foregroundStyle(.secondary)
                        .monospacedDigit()
                }
            }
            Spacer(minLength: 0)
        }
        .padding(.vertical, 4)
        .accessibilityElement(children: .combine)
    }

    private var artwork: some View {
        Group {
            if let url = episode.imageURL ?? showImageURL {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 44, height: 44)) { phase in
                    switch phase {
                    case .success(let image):
                        image.resizable().scaledToFill()
                    default:
                        artworkPlaceholder
                    }
                }
            } else {
                artworkPlaceholder
            }
        }
        .frame(width: 44, height: 44)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private var artworkPlaceholder: some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
            .fill(Color.secondary.opacity(0.18))
            .overlay(
                Image(systemName: "waveform")
                    .font(.system(size: 18, weight: .light))
                    .foregroundStyle(.secondary)
            )
    }

    private var formattedRuntime: String? {
        guard let duration = episode.duration, duration > 0 else { return nil }
        return PlayerTimeFormat.clock(duration)
    }
}
