import SwiftUI

// MARK: - ClipsSegment
//
// Every clip the user has made, newest first, bucketed into Today / This Week /
// Earlier. Tap a card to seek and play; swipe or long-press for delete.
struct ClipsSegment: View {

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    let searchQuery: String
    let onOpenEpisode: (UUID) -> Void
    let onPullToSearch: () -> Void

    var body: some View {
        let all = store.allClips()
        let filteredClips = filtered(all)

        List {
            Text("Clips")
                .font(.system(size: 34, weight: .bold))
                .frame(maxWidth: .infinity, alignment: .leading)
                .listRowInsets(EdgeInsets(
                    top: AppTheme.Spacing.sm,
                    leading: AppTheme.Spacing.md,
                    bottom: AppTheme.Spacing.sm,
                    trailing: AppTheme.Spacing.md
                ))
                .listRowSeparator(.hidden)
                .listRowBackground(Color(.systemBackground))
                .accessibilityAddTraits(.isHeader)

            if !all.isEmpty, !filteredClips.isEmpty {
                clipSections(buckets(from: filteredClips))
            }
        }
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
        .background(Color(.systemBackground))
        .scrollBounceBehavior(.always)
        .overlay {
            if all.isEmpty {
                emptyState
            } else if filteredClips.isEmpty {
                ContentUnavailableView.search(text: searchQuery)
            }
        }
        .onScrollGeometryChange(for: Bool.self) { geometry in
            geometry.contentOffset.y + geometry.contentInsets.top < -44
        } action: { _, pulledPastSearchThreshold in
            if pulledPastSearchThreshold {
                onPullToSearch()
            }
        }
    }

    // MARK: - List

    @ViewBuilder
    private func clipSections(_ sections: [(String, [Clip])]) -> some View {
        ForEach(sections, id: \.0) { sectionName, clips in
            Section {
                ForEach(clips) { clip in
                    clipRow(clip)
                }
            } header: {
                Text(sectionName)
                    .font(.system(.caption, design: .rounded).weight(.semibold))
                    .tracking(0.6)
                    .textCase(.uppercase)
                    .foregroundStyle(.secondary)
            }
        }
    }

    @ViewBuilder
    private func clipRow(_ clip: Clip) -> some View {
        let episode = store.episode(id: clip.episodeID)
        let podcast = store.podcast(id: clip.subscriptionID)
        ClippingsCard(
            clip: clip,
            episode: episode,
            podcast: podcast,
            onPlay: { playClip(clip, episode: episode) },
            onOpenEpisode: {
                if let episode { onOpenEpisode(episode.id) }
            },
            onDelete: {
                Haptics.delete()
                store.deleteClip(id: clip.id)
            }
        )
        .listRowInsets(EdgeInsets(top: 6, leading: 16, bottom: 6, trailing: 16))
        .listRowBackground(Color.clear)
        .listRowSeparator(.hidden)
        .swipeActions(edge: .trailing, allowsFullSwipe: false) {
            Button(role: .destructive) {
                Haptics.delete()
                store.deleteClip(id: clip.id)
            } label: {
                Label("Delete", systemImage: "trash")
            }
        }
    }

    // MARK: - Empty state

    private var emptyState: some View {
        ContentUnavailableView {
            Label("No Clips Yet", systemImage: "scissors")
        } description: {
            Text("Long-press any transcript line to clip a moment, or use your headphones' clip button while listening.")
        }
    }

    // MARK: - Play

    private func playClip(_ clip: Clip, episode: Episode?) {
        guard let episode else { return }
        playback.setEpisode(episode)
        playback.seek(to: clip.startSeconds)
        playback.play()
        NotificationCenter.default.post(name: .openPlayerRequested, object: nil)
    }

    // MARK: - Derived

    private func filtered(_ clips: [Clip]) -> [Clip] {
        guard !searchQuery.isEmpty else { return clips }
        let q = searchQuery.lowercased()
        return clips.filter {
            $0.transcriptText.lowercased().contains(q)
            || ($0.caption?.lowercased().contains(q) ?? false)
            || (store.episode(id: $0.episodeID)?.title.lowercased().contains(q) ?? false)
            || (store.podcast(id: $0.subscriptionID)?.title.lowercased().contains(q) ?? false)
        }
    }

    private func buckets(from clips: [Clip]) -> [(String, [Clip])] {
        let now = Date()
        var today: [Clip] = []
        var thisWeek: [Clip] = []
        var earlier: [Clip] = []
        for clip in clips {
            let age = now.timeIntervalSince(clip.createdAt)
            if age < 86_400 { today.append(clip) }
            else if age < 7 * 86_400 { thisWeek.append(clip) }
            else { earlier.append(clip) }
        }
        return [("Today", today), ("This Week", thisWeek), ("Earlier", earlier)]
            .filter { !$0.1.isEmpty }
    }
}
