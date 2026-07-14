import SwiftUI

// MARK: - PlayerGenerationSourceChip
//
// Shown in the player's episode header when the playing episode was generated
// by the agent. Surfaces the source: either the in-app chat conversation that
// triggered generation, or the Nostr peer who requested it. Tapping dismisses
// the player and navigates to the originating conversation.

struct PlayerGenerationSourceChip: View {

    let source: Episode.GenerationSource
    @Environment(AppStateStore.self) private var store

    var body: some View {
        Button(action: openSource) {
            HStack(spacing: AppTheme.Spacing.xs) {
                leadingIcon
                VStack(alignment: .leading, spacing: 1) {
                    Text("GENERATED FROM".uppercased())
                        .font(.system(size: 9, weight: .semibold, design: .rounded))
                        .tracking(0.8)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Text(sourceLabel)
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

    // MARK: - Subviews

    @ViewBuilder
    private var leadingIcon: some View {
        switch source {
        case .inAppChat:
            Image(systemName: "sparkles")
                .font(.caption2.weight(.semibold))
                .foregroundStyle(Color.accentColor)

        case .nostr(_, let pubkeyHex):
            if let url = nostrProfile(for: pubkeyHex)?.pictureURL {
                CachedAsyncImage(url: url) { phase in
                    if case .success(let img) = phase {
                        img.resizable().scaledToFill()
                    } else {
                        nostrFallbackIcon
                    }
                }
                .frame(width: 16, height: 16)
                .clipShape(Circle())
            } else {
                nostrFallbackIcon
            }
        }
    }

    private var nostrFallbackIcon: some View {
        Image(systemName: "person.crop.circle")
            .font(.caption2.weight(.semibold))
            .foregroundStyle(Color.accentColor)
    }

    // MARK: - Helpers

    private var sourceLabel: String {
        switch source {
        case .inAppChat:
            return "Your Conversation"
        case .nostr(_, let pubkeyHex):
            if let profile = nostrProfile(for: pubkeyHex), let label = profile.bestLabel {
                return label
            }
            return "Nostr Conversation"
        }
    }

    private func nostrProfile(for pubkeyHex: String) -> NostrProfileMetadata? {
        store.state.nostrProfileCache[pubkeyHex]
    }

    private func openSource() {
        Haptics.selection()
        switch source {
        case .inAppChat(let conversationID):
            NotificationCenter.default.post(
                name: .openAgentChatConversation,
                object: nil,
                userInfo: ["conversationID": conversationID]
            )
        case .nostr(let rootEventID, _):
            NotificationCenter.default.post(
                name: .openNostrConversationRequested,
                object: nil,
                userInfo: ["rootEventID": rootEventID]
            )
        }
    }
}
