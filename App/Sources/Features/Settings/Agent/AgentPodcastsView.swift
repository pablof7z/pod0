import SwiftUI

// MARK: - AgentPodcastsView
//
// Lists agent-owned podcasts (shows created via the `create_podcast` tool or
// the AI agent) and lets the user toggle per-podcast Nostr visibility between
// private (library only) and public (published as NIP-F4 kind:10154 show events
// via the Rust kernel's `podcast.publish` namespace).

struct AgentPodcastsView: View {
    @Environment(AppStateStore.self) private var store

    private var ownedPodcasts: [Podcast] {
        store.rustOwnedPodcasts()
    }

    var body: some View {
        List {
            if ownedPodcasts.isEmpty {
                emptyState
            } else {
                podcastsSection
            }
        }
        .settingsListStyle()
        .navigationTitle("Agent Podcasts")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sections

    private var podcastsSection: some View {
        Section {
            ForEach(ownedPodcasts) { podcast in
                AgentPodcastRow(podcast: podcast)
            }
        } header: {
            Text("Owned Shows")
        } footer: {
            Text("Public shows are published as NIP-F4 Nostr events — the canonical podcast standard — signed by a per-show key. Episodes added to public shows are also published when Nostr is enabled.")
        }
    }

    private var emptyState: some View {
        Section {
            VStack(spacing: 12) {
                Image(systemName: "waveform.badge.plus")
                    .font(.system(size: 40))
                    .foregroundStyle(.secondary)
                Text("No agent-owned podcasts yet")
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.secondary)
                Text("Ask the agent to create a podcast using the create_podcast tool.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.tertiary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, 24)
        }
    }

}

// MARK: - AgentPodcastRow

private struct AgentPodcastRow: View {
    @Environment(AppStateStore.self) private var store
    let podcast: Podcast

    private var isPublic: Bool { podcast.nostrVisibility == .public }

    var body: some View {
        HStack(spacing: 12) {
            CachedAsyncImage(url: podcast.imageURL, targetSize: CGSize(width: 44, height: 44)) { phase in
                switch phase {
                case .success(let img):
                    img.resizable().aspectRatio(contentMode: .fill)
                default:
                    RoundedRectangle(cornerRadius: 8).fill(Color(.systemFill))
                        .overlay(Image(systemName: "mic.fill").foregroundStyle(.secondary))
                }
            }
            .frame(width: 44, height: 44)
            .clipShape(RoundedRectangle(cornerRadius: 8))

            VStack(alignment: .leading, spacing: 2) {
                Text(podcast.title)
                    .font(AppTheme.Typography.callout)
                    .lineLimit(1)
                if !podcast.author.isEmpty {
                    Text(podcast.author)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                }
                Text(isPublic ? "Public · Nostr" : "Private")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(isPublic ? Color.accentColor : .secondary)
            }

            Spacer()

            Toggle("", isOn: Binding(
                get: { isPublic },
                set: { newValue in
                    Haptics.selection()
                    let podcastID = podcast.id.uuidString
                    Task {
                        let mgr = LiveAgentOwnedPodcastManager(store: store)
                        _ = try? await mgr.updatePodcast(
                            podcastID: podcastID,
                            title: nil, description: nil, author: nil,
                            imageURL: nil, visibility: newValue ? .public : .private
                        )
                    }
                }
            ))
            .labelsHidden()
        }
        .accessibilityLabel("\(podcast.title), \(isPublic ? "public" : "private")")
        .accessibilityHint("Toggle Nostr visibility")
    }
}
