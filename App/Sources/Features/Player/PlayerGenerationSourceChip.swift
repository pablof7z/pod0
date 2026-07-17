import SwiftUI

// MARK: - PlayerGenerationSourceChip
//
// Shown in the player's episode header when the playing episode was generated
// by the agent from an in-app chat conversation. Tapping dismisses the player
// and navigates to the originating conversation.

struct PlayerGenerationSourceChip: View {

    let source: Episode.GenerationSource

    var body: some View {
        Button(action: openSource) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Image(systemName: "sparkles")
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(Color.accentColor)
                VStack(alignment: .leading, spacing: 1) {
                    Text("GENERATED FROM".uppercased())
                        .font(.system(size: 9, weight: .semibold, design: .rounded))
                        .tracking(0.8)
                        .foregroundStyle(.secondary)
                        .lineLimit(1)
                    Text("Your Conversation")
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

    // MARK: - Helpers

    private func openSource() {
        Haptics.selection()
        switch source {
        case .inAppChat(let conversationID):
            NotificationCenter.default.post(
                name: .openAgentChatConversation,
                object: nil,
                userInfo: ["conversationID": conversationID]
            )
        }
    }
}
