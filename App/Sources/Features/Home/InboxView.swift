import SwiftUI

// MARK: - InboxView

/// Full-screen list of all episodes the AI agent routed to the inbox.
/// Shows every unplayed `.inbox` episode (no cap), with the agent's
/// rationale beneath each title, scoped to the active category when set.
struct InboxView: View {
    let allowedSubscriptionIDs: Set<UUID>?

    @Environment(AppStateStore.self) private var store
    @Environment(PlaybackState.self) private var playback

    var body: some View {
        List {
            ForEach(inboxEpisodes) { ep in
                InboxRow(
                    episode: ep,
                    podcast: store.podcast(id: ep.podcastID),
                    onPlay: { playEpisode(ep) }
                )
                .swipeActions(edge: .leading, allowsFullSwipe: true) {
                    EpisodeRowLeadingSwipeAction(episode: ep, playback: playback)
                }
                .listRowInsets(EdgeInsets(
                    top: AppTheme.Spacing.sm,
                    leading: AppTheme.Spacing.md,
                    bottom: AppTheme.Spacing.sm,
                    trailing: AppTheme.Spacing.md
                ))
                .listRowBackground(Color(.secondarySystemBackground))
            }
        }
        .listStyle(.insetGrouped)
        .navigationTitle("Inbox")
        .navigationBarTitleDisplayMode(.inline)
        .background(Color(.systemGroupedBackground).ignoresSafeArea())
        .overlay {
            if inboxEpisodes.isEmpty {
                ContentUnavailableView(
                    "All Caught Up",
                    systemImage: "tray",
                    description: Text("No episodes in your inbox right now.")
                )
            }
        }
    }

    private var inboxEpisodes: [Episode] {
        store.state.episodes
            .filter { ep in
                guard ep.triageDecision == .inbox, !ep.played else { return false }
                if let allowed = allowedSubscriptionIDs,
                   !allowed.contains(ep.podcastID) { return false }
                return true
            }
            .sorted { lhs, rhs in
                // Hero-flagged episode first, then newest pubDate
                if lhs.triageIsHero != rhs.triageIsHero { return lhs.triageIsHero }
                return lhs.pubDate > rhs.pubDate
            }
    }

    private func playEpisode(_ episode: Episode) {
        Haptics.medium()
        playback.setEpisode(episode)
        playback.play()
    }
}

// MARK: - InboxRow

/// List row for a single inbox episode: 52-pt artwork, show + title,
/// and the agent's rationale ("Because …") in a muted italic below.
private struct InboxRow: View {
    let episode: Episode
    let podcast: Podcast?
    let onPlay: () -> Void

    @Environment(AppStateStore.self) private var store

    var body: some View {
        Button(action: onPlay) {
            HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
                artwork
                VStack(alignment: .leading, spacing: 4) {
                    episodeMeta
                    if let rationale = rationaleText {
                        Text(rationale)
                            .font(AppTheme.Typography.subheadline)
                            .italic()
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.leading)
                    }
                }
            }
            .contentShape(Rectangle())
        }
        .buttonStyle(.plain)
        .contextMenu {
            EpisodeRowContextMenu(
                episode: episode,
                store: store,
                openDetailsRoute: HomeEpisodeRoute(episodeID: episode.id)
            )
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
        .accessibilityHint("Plays this episode")
    }

    private var artwork: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous)
                .fill(Color(.tertiarySystemFill))
            let url = episode.imageURL ?? podcast?.imageURL
            if let url {
                CachedAsyncImage(url: url, targetSize: CGSize(width: 104, height: 104)) { phase in
                    switch phase {
                    case .success(let image): image.resizable().scaledToFill()
                    default:
                        Image(systemName: "waveform")
                            .font(.system(size: 18, weight: .light))
                            .foregroundStyle(.secondary)
                    }
                }
            } else {
                Image(systemName: "waveform")
                    .font(.system(size: 18, weight: .light))
                    .foregroundStyle(.secondary)
            }
        }
        .frame(width: 52, height: 52)
        .clipShape(RoundedRectangle(cornerRadius: AppTheme.Corner.sm, style: .continuous))
    }

    private var episodeMeta: some View {
        VStack(alignment: .leading, spacing: 2) {
            if let showName = podcast?.title, !showName.isEmpty {
                Text(showName)
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
                    .lineLimit(1)
            }
            Text(episode.title)
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
                .lineLimit(2)
                .multilineTextAlignment(.leading)
        }
    }

    private var rationaleText: String? {
        guard let raw = episode.triageRationale, !raw.isEmpty else { return nil }
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        if trimmed.lowercased().hasPrefix("because") { return trimmed }
        return "Because \(trimmed.first.map { String($0).lowercased() + trimmed.dropFirst() } ?? "")"
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if let s = podcast?.title, !s.isEmpty { parts.append(s) }
        parts.append(episode.title)
        if let r = episode.triageRationale, !r.isEmpty { parts.append(r) }
        return parts.joined(separator: ", ")
    }
}
