import SwiftUI

// MARK: - HomeThreadedTodayView
//
// Half-sheet presented from the Home featured "Threaded Today" pill.
// Renders a topic header + a virtual playlist where each row is a
// `ThreadingMention` (an `(episodeID, segmentStart, segmentEnd)` triple).
//
// Tapping a row seeks to that mention's `startMS` inside its episode and
// starts playback. "Play through" enqueues every mention's episode into
// `PlaybackState.queue` and starts the first one — the player then walks
// the queue naturally as each episode finishes.
//
// "Save as briefing" hands off to `BriefingComposeSheet` with the topic
// pre-loaded as the freeform query + `BriefingScope.thisTopic` selected.
// The full playlist isn't pre-injected into the briefing model — the
// briefing pipeline builds its own segment plan from the topic; pre-
// loading the playlist would force the compose surface to grow a new
// "explicit segment list" input we don't need today.

struct HomeThreadedTodayView: View {

    let active: ThreadingInferenceService.ActiveTopic

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback
    @Environment(\.dismiss) private var dismiss

    @State private var showBriefingCompose: Bool = false

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                    header
                    actionRow
                    Divider()
                    playlist
                }
                .padding(AppTheme.Spacing.md)
            }
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .navigationTitle("Threaded Today")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") {
                        Haptics.selection()
                        dismiss()
                    }
                }
            }
            .sheet(isPresented: $showBriefingCompose) {
                BriefingComposeSheet(
                    onCompose: { _ in
                        // The Briefings tab actually composes; from here we
                        // just hand off the request. Composing in-place
                        // would duplicate `BriefingsViewModel` state with
                        // no UX win — the user goes back to Home or to
                        // Briefings to see the result either way.
                        showBriefingCompose = false
                    },
                    initialFreeformQuery: active.topic.displayName,
                    initialScope: .thisTopic
                )
                .presentationDetents([.medium, .large])
            }
        }
    }

    // MARK: - Header

    private var header: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text(active.topic.displayName)
                .font(AppTheme.Typography.title)
                .foregroundStyle(.primary)
            if let def = active.topic.definition, !def.isEmpty {
                Text(def)
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            Text("\(rows.count) mentions across \(distinctEpisodeCount) unplayed episodes")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.tertiary)
        }
    }

    // MARK: - Actions

    private var actionRow: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Button {
                Haptics.medium()
                playThroughAll()
            } label: {
                Label("Play through", systemImage: "play.fill")
                    .font(.subheadline.weight(.semibold))
                    .padding(.vertical, AppTheme.Spacing.sm)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.borderedProminent)
            .tint(AppTheme.Tint.agentSurface)

            Button {
                Haptics.light()
                showBriefingCompose = true
            } label: {
                Label("Save as briefing", systemImage: "tray.and.arrow.down")
                    .font(.subheadline.weight(.semibold))
                    .padding(.vertical, AppTheme.Spacing.sm)
                    .frame(maxWidth: .infinity)
            }
            .buttonStyle(.bordered)
        }
    }

    // MARK: - Playlist

    private var playlist: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            ForEach(rows) { row in
                Button {
                    Haptics.selection()
                    playMention(row)
                } label: {
                    HomeThreadedTodayRow(row: row)
                }
                .buttonStyle(.plain)
            }
        }
    }

    // MARK: - Derivation

    /// Mentions of this topic restricted to *unplayed* episodes, in playback
    /// (publish-newest-first) order. Drops dead-id mentions silently —
    /// `threadingMentions(forTopic:)` already filters those, but we also
    /// re-check `played` here because the store accessor doesn't.
    private var rows: [Row] {
        // AI Inbox: archived episodes are silently soft-hidden from threading topics.
        let unplayedIDs = Set(store.state.episodes.filter { !$0.played && !$0.isTriageArchived }.map(\.id))
        return store.threadingMentions(forTopic: active.topic.id)
            .filter { unplayedIDs.contains($0.episodeID) }
            .compactMap { mention in
                guard let ep = store.episode(id: mention.episodeID) else { return nil }
                return Row(
                    mention: mention,
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID)
                )
            }
    }

    private var distinctEpisodeCount: Int {
        Set(rows.map(\.episode.id)).count
    }

    // MARK: - Playback

    private func playMention(_ row: Row) {
        playback.setEpisode(row.episode)
        playback.seek(to: TimeInterval(row.mention.startMS) / 1_000)
        playback.play()
        dismiss()
    }

    /// Walk every distinct episode in the playlist: the first one starts
    /// playing (seeking to its first mention's `startMS`); the rest are
    /// appended to the Up Next queue in publish order. The queue walks
    /// naturally on episode-finish via the existing `playNext` flow.
    private func playThroughAll() {
        let uniqueEpisodes = NSOrderedSet(array: rows.map { $0.episode.id }).array.compactMap { $0 as? UUID }
        guard let firstID = uniqueEpisodes.first,
              let first = store.episode(id: firstID) else { return }
        let firstMention = rows.first { $0.episode.id == firstID }

        playback.setEpisode(first)
        if let mention = firstMention {
            playback.seek(to: TimeInterval(mention.mention.startMS) / 1_000)
        }
        playback.play()

        for id in uniqueEpisodes.dropFirst() {
            playback.enqueue(id)
        }
        dismiss()
    }

    // MARK: - Row model

    struct Row: Identifiable {
        let mention: ThreadingMention
        let episode: Episode
        let podcast: Podcast?
        var id: UUID { mention.id }
    }
}

// MARK: - HomeThreadedTodayRow

private struct HomeThreadedTodayRow: View {
    let row: HomeThreadedTodayView.Row

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.md) {
            VStack(alignment: .leading, spacing: 2) {
                if let title = row.podcast?.title, !title.isEmpty {
                    Text(title)
                        .font(AppTheme.Typography.caption)
                        .tracking(0.6)
                        .textCase(.uppercase)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(row.episode.title)
                    .font(AppTheme.Typography.headline)
                    .lineLimit(2)
                Text(row.mention.snippet)
                    .font(AppTheme.Typography.subheadline)
                    .italic()
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
                    .padding(.top, AppTheme.Spacing.xs)
            }
            Spacer(minLength: AppTheme.Spacing.sm)
            VStack(alignment: .trailing, spacing: AppTheme.Spacing.xs) {
                Text(row.mention.formattedTimestamp)
                    .font(AppTheme.Typography.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
                Image(systemName: "play.circle")
                    .font(.title3)
                    .foregroundStyle(AppTheme.Tint.agentSurface)
            }
        }
        .padding(AppTheme.Spacing.md)
        .background(
            Color(.secondarySystemBackground),
            in: RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
        )
    }
}
