import SwiftUI

// MARK: - PlayerClipSourceChip
//
// Appears in the player's floating chrome when the active chapter has a
// `sourceEpisodeID` — i.e. the current audio is a clip from another episode
// synthesised into an agent-generated podcast. Shows the source show + episode
// title and lets the user tap to open the originating episode detail.

struct PlayerClipSourceChip: View {

    let sourceEpisodeID: String
    @Environment(AppStateStore.self) private var store

    var body: some View {
        if let episode = resolvedEpisode {
            Button(action: openSource) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    Image(systemName: "quote.bubble.fill")
                        .font(.caption2.weight(.semibold))
                        .foregroundStyle(Color.accentColor)
                    VStack(alignment: .leading, spacing: 1) {
                        if let showName {
                            Text(showName)
                                .font(.system(size: 9, weight: .semibold, design: .rounded))
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                        Text(episode.title)
                            .font(.caption.weight(.medium))
                            .foregroundStyle(.primary)
                            .lineLimit(1)
                    }
                    Spacer(minLength: 0)
                    Image(systemName: "arrow.up.forward.circle")
                        .font(.caption.weight(.medium))
                        .foregroundStyle(.secondary)
                }
                .padding(.horizontal, AppTheme.Spacing.sm)
                .padding(.vertical, AppTheme.Spacing.xs)
                .glassSurface(
                    cornerRadius: AppTheme.Corner.md,
                    tint: Color.accentColor.opacity(0.08)
                )
            }
            .buttonStyle(.plain)
            .transition(.opacity.combined(with: .move(edge: .top)))
        }
    }

    private var resolvedEpisode: Episode? {
        guard let uuid = UUID(uuidString: sourceEpisodeID) else { return nil }
        return store.episode(id: uuid)
    }

    private var showName: String? {
        guard let episode = resolvedEpisode else { return nil }
        return store.podcast(id: episode.podcastID)?.title
    }

    private func openSource() {
        Haptics.selection()
        NotificationCenter.default.post(
            name: .openEpisodeDetailRequested,
            object: nil,
            userInfo: ["episodeID": sourceEpisodeID]
        )
    }
}
