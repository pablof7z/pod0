import SwiftUI

struct SharedAgentChatTranscript: View {
    let messages: [ChatMessage]
    let streamingContent: String?
    let isRunning: Bool
    var onOpenRecallEvidence: (RecallEvidence) -> Void = { _ in }
    @State private var scrolledMessageID: AnyHashable?

    var body: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
                    ForEach(messages) { message in
                        AgentChatBubble(
                            message: message,
                            onOpenRecallEvidence: onOpenRecallEvidence
                        )
                        .id(message.id)
                    }
                    if isRunning {
                        if let streamingContent, !streamingContent.isEmpty {
                            AgentChatBubble(message: ChatMessage(
                                role: .assistant,
                                text: streamingContent
                            ))
                        } else {
                            AgentTypingIndicator(toolName: nil)
                        }
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.md)
            }
            .scrollDismissesKeyboard(.interactively)
            .scrollPosition(id: $scrolledMessageID, anchor: .bottom)
            .defaultScrollAnchor(.bottom)
            .onChange(of: messages.count) { _, _ in
                if let last = messages.last {
                    withAnimation(AppTheme.Animation.spring) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            .onChange(of: streamingContent) { _, _ in
                guard let last = messages.last else { return }
                proxy.scrollTo(last.id, anchor: .bottom)
            }
        }
    }

}
