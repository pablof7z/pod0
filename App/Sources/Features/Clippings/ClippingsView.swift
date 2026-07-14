import SwiftUI

// MARK: - ClippingsView

/// Global Clippings feed — every clip the user has made, newest first,
/// bucketed into Today / This Week / Earlier. Tap a card to seek and play;
/// long-press for share / open-episode / delete.
struct ClippingsView: View {

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    @State private var searchQuery = ""
    @State private var episodeNavTarget: EpisodeNavTarget?

    var body: some View {
        clipsContent
            .navigationTitle("Clippings")
            .navigationBarTitleDisplayMode(.large)
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
            .searchable(text: $searchQuery, prompt: "Search clips")
            .navigationDestination(item: $episodeNavTarget) { target in
                EpisodeDetailView(episodeID: target.id)
            }
    }

    // MARK: - Content

    @ViewBuilder
    private var clipsContent: some View {
        let all = store.allClips()
        if all.isEmpty {
            emptyFirst
        } else {
            let filtered = filtered(all)
            if filtered.isEmpty {
                ContentUnavailableView.search(text: searchQuery)
            } else {
                clipsList(buckets(from: filtered))
            }
        }
    }

    // MARK: - List

    private func clipsList(_ sections: [(String, [Clip])]) -> some View {
        List {
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
        .listStyle(.plain)
        .scrollContentBackground(.hidden)
    }

    @ViewBuilder
    private func clipRow(_ clip: Clip) -> some View {
        if let episode = store.episode(id: clip.episodeID) {
            let podcast = store.podcast(id: clip.subscriptionID)
            ClippingsCard(
                clip: clip,
                episode: episode,
                podcast: podcast,
                onPlay: { playClip(clip, episode: episode) },
                onOpenEpisode: {
                    episodeNavTarget = EpisodeNavTarget(id: episode.id)
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
    }

    // MARK: - Empty state

    private var emptyFirst: some View {
        ContentUnavailableView {
            Label("No Clippings Yet", systemImage: "scissors")
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

// MARK: - EpisodeNavTarget

/// Thin Identifiable wrapper so a UUID can drive `.navigationDestination(item:)`.
private struct EpisodeNavTarget: Identifiable, Hashable {
    let id: UUID
}

// MARK: - Preview

#Preview {
    let store = AppStateStore()
    let podcast = Podcast(
        feedURL: URL(string: "https://example.com/feed")!,
        title: "The Peter Attia Drive"
    )
    let episode = Episode(
        podcastID: podcast.id,
        guid: "preview",
        title: "How to Think About Keto",
        pubDate: Date(),
        enclosureURL: URL(string: "https://example.com/x.mp3")!
    )
    store.state.podcasts = [podcast]
    store.state.subscriptions = [PodcastSubscription(podcastID: podcast.id)]
    store.state.episodes = [episode]
    store.addClip(Clip(
        episodeID: episode.id,
        subscriptionID: podcast.id,
        startMs: 14 * 60_000 + 31_000,
        endMs: 14 * 60_000 + 58_000,
        caption: "On metabolism",
        transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
        source: .touch
    ))
    store.addClip(Clip(
        episodeID: episode.id,
        subscriptionID: podcast.id,
        startMs: 32 * 60_000,
        endMs: 32 * 60_000 + 15_000,
        transcriptText: "Zone 2 training is the bedrock of aerobic capacity.",
        source: .auto
    ))
    return NavigationStack {
        ClippingsView()
            .environment(store)
            .environment(PlaybackState())
    }
}
