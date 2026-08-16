import SwiftUI
import Pod0Core

struct AgentAssistantMessageView: View {
    let message: ChatMessage
    let onRegenerate: (() -> Void)?
    let onOpenEvidence: (RecallEvidenceProjection, UUID) -> Void

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            AgentAvatarView()
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                messageText
                if !message.recallEvidence.isEmpty {
                    ForEach(message.recallEvidence, id: \.spanId) { evidence in
                        RecallEvidenceCard(evidence: evidence) {
                            onOpenEvidence(evidence, message.id)
                        }
                    }
                }
                Text(message.timestamp, style: .time)
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.tertiary)
                    .padding(.leading, 14)
            }
            Spacer(minLength: 0)
        }
    }

    private var messageText: some View {
        MarkdownView(text: message.text)
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(.regular, in: .rect(cornerRadius: 18))
            .contextMenu {
                Button {
                    UIPasteboard.general.string = message.text
                    Haptics.selection()
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                if let onRegenerate {
                    Divider()
                    Button {
                        Haptics.selection()
                        onRegenerate()
                    } label: {
                        Label("Regenerate Response", systemImage: "arrow.clockwise")
                    }
                }
            }
    }
}
