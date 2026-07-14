import SwiftUI

// MARK: - FeedbackBubble

/// Chat bubble for a single feedback message. Renders the body as
/// markdown (line-by-line so newlines survive and inline `**bold**`,
/// `*italic*`, `` `code` `` and `[link](url)` are interpreted), lifts
/// any standalone https image URLs out into AsyncImage attachments, and
/// optionally shows a 28pt author avatar + display-name header on the
/// first message of each "burst" (sender change or > 5 min gap).
struct FeedbackBubble: View {
    let content: String
    let isFromMe: Bool
    let createdAt: Date
    var displayName: String? = nil
    var pictureURL: URL? = nil
    var avatarInitial: String? = nil
    var showHeader: Bool = false
    var onQuoteReply: (() -> Void)? = nil

    private enum Layout {
        static let spacerMinLength: CGFloat = 60
        static let bubbleCornerRadius: CGFloat = 18
        static let bubblePaddingH: CGFloat = 12
        static let rowPaddingV: CGFloat = 2
        static let avatarSize: CGFloat = 28
        static let imageMaxWidth: CGFloat = 260
        static let imageCornerRadius: CGFloat = 12
        static let imagePlaceholderHeight: CGFloat = 120
    }

    private static let imageExtensions: Set<String> = ["jpg", "jpeg", "png", "gif", "webp", "heic"]

    var body: some View {
        HStack(alignment: .bottom, spacing: AppTheme.Spacing.xs) {
            if isFromMe {
                Spacer(minLength: Layout.spacerMinLength)
            } else {
                avatarSlot
            }

            VStack(alignment: isFromMe ? .trailing : .leading, spacing: AppTheme.Spacing.xs) {
                if showHeader, let name = displayName, !name.isEmpty {
                    Text(name)
                        .font(AppTheme.Typography.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                        .padding(.horizontal, AppTheme.Spacing.xs)
                }

                if !textOnlyContent.isEmpty {
                    if isFromMe { myBubble } else { theirBubble }
                }

                ForEach(imageURLs, id: \.absoluteString) { url in
                    asyncImage(url)
                }

                Text(createdAt, style: .time)
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
            }

            if isFromMe {
                avatarSlot
            } else {
                Spacer(minLength: Layout.spacerMinLength)
            }
        }
        .padding(.horizontal, AppTheme.Spacing.md)
        .padding(.vertical, Layout.rowPaddingV)
    }

    // MARK: - Avatar slot

    @ViewBuilder
    private var avatarSlot: some View {
        if showHeader {
            FeedbackAuthorAvatar(
                pictureURL: pictureURL,
                initial: avatarInitial ?? "?",
                size: Layout.avatarSize
            )
        } else {
            Color.clear.frame(width: Layout.avatarSize, height: 1)
        }
    }

    // MARK: - Bubbles

    private var myBubble: some View {
        Text(markdownContent)
            .padding(.horizontal, Layout.bubblePaddingH)
            .padding(.vertical, AppTheme.Spacing.sm)
            .glassEffect(.regular.tint(.accentColor), in: .rect(cornerRadius: Layout.bubbleCornerRadius))
            .foregroundStyle(.white)
            .multilineTextAlignment(.leading)
            .copyableTextMenu(content)
    }

    private var theirBubble: some View {
        Text(markdownContent)
            .padding(.horizontal, Layout.bubblePaddingH)
            .padding(.vertical, AppTheme.Spacing.sm)
            .background(Color(.secondarySystemBackground), in: .rect(cornerRadius: Layout.bubbleCornerRadius))
            .foregroundStyle(.primary)
            .multilineTextAlignment(.leading)
            .contextMenu {
                Button {
                    UIPasteboard.general.string = content
                    Haptics.selection()
                } label: {
                    Label("Copy", systemImage: "doc.on.doc")
                }
                if let onQuoteReply {
                    Button {
                        onQuoteReply()
                    } label: {
                        Label("Reply", systemImage: "arrowshape.turn.up.left")
                    }
                }
            }
    }

    // MARK: - Inline image

    private func asyncImage(_ url: URL) -> some View {
        AsyncImage(url: url) { phase in
            switch phase {
            case .success(let image):
                image.resizable().scaledToFit()
            case .failure:
                Label("Image unavailable", systemImage: "photo")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .padding(AppTheme.Spacing.sm)
            default:
                Color.secondary.opacity(0.15)
                    .frame(height: Layout.imagePlaceholderHeight)
            }
        }
        .frame(maxWidth: Layout.imageMaxWidth)
        .clipShape(.rect(cornerRadius: Layout.imageCornerRadius))
    }

    // MARK: - Content parsing

    /// URLs on standalone lines whose path extension looks like an
    /// image. These are pulled out of the text body and rendered as
    /// AsyncImage attachments below the bubble.
    private var imageURLs: [URL] {
        content.components(separatedBy: "\n").compactMap { line in
            let t = line.trimmingCharacters(in: .whitespaces)
            guard let url = URL(string: t),
                  let scheme = url.scheme?.lowercased(),
                  scheme == "https" || scheme == "http",
                  Self.imageExtensions.contains(url.pathExtension.lowercased())
            else { return nil }
            return url
        }
    }

    private var textOnlyContent: String {
        let imageLines = Set(imageURLs.map(\.absoluteString))
        return content
            .components(separatedBy: "\n")
            .filter { !imageLines.contains($0.trimmingCharacters(in: .whitespaces)) }
            .joined(separator: "\n")
            .trimmingCharacters(in: .whitespacesAndNewlines)
    }

    /// Parses each non-image line through `AttributedString(markdown:)`
    /// with `.inlineOnlyPreservingWhitespace` so newlines from the LLM
    /// survive AND inline syntax on each line is interpreted. A single
    /// `AttributedString(markdown:)` over the full body collapses past
    /// the first paragraph and would lose most chat-style formatting.
    private var markdownContent: AttributedString {
        let opts = AttributedString.MarkdownParsingOptions(
            interpretedSyntax: .inlineOnlyPreservingWhitespace
        )
        var combined = AttributedString("")
        let lines = textOnlyContent.split(separator: "\n", omittingEmptySubsequences: false)
        for (i, line) in lines.enumerated() {
            let str = String(line)
            let parsed = (try? AttributedString(markdown: str, options: opts)) ?? AttributedString(str)
            combined.append(parsed)
            if i < lines.count - 1 {
                combined.append(AttributedString("\n"))
            }
        }
        return combined
    }
}
