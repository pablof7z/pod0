import SwiftUI

// MARK: - FeedbackThreadRow

struct FeedbackThreadRow: View {

    private enum Layout {
        static let rowSpacing: CGFloat = 12
        static let contentSpacing: CGFloat = 4
        static let badgeCornerRadius: CGFloat = 8
        static let badgeSize: CGFloat = 29
        static let badgeIconSize: CGFloat = 14
        static let metadataSpacing: CGFloat = 6
        static let rowVerticalPadding: CGFloat = 10
    }

    let thread: FeedbackThread
    var query: String = ""
    /// When non-nil, rendered as "by <name>" in the metadata row. Only
    /// passed for non-local authors in `Everyone` mode so own threads
    /// stay anonymous-looking.
    var authorName: String? = nil

    var body: some View {
        HStack(spacing: Layout.rowSpacing) {
            categoryBadge

            VStack(alignment: .leading, spacing: Layout.contentSpacing) {
                HStack(alignment: .firstTextBaseline) {
                    titleText
                        .lineLimit(1)
                        .font(AppTheme.Typography.body.weight(thread.title != nil ? .semibold : .regular))
                    Spacer()
                    Text(thread.createdAt, style: .relative)
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.secondary)
                }

                if let summary = thread.summary, !summary.isEmpty {
                    subtitleText(summary)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                } else if thread.title != nil, !thread.content.isEmpty {
                    subtitleText(thread.content)
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }

                HStack(spacing: Layout.metadataSpacing) {
                    if let status = thread.statusLabel, !status.isEmpty {
                        FeedbackStatusBadge(status: status)
                    }

                    if !thread.replies.isEmpty {
                        Label("\(thread.replies.count)", systemImage: "bubble.left")
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(.secondary)
                    }

                    if let authorName, !authorName.isEmpty {
                        Text("by \(authorName)")
                            .font(AppTheme.Typography.caption2)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                }
            }
        }
        .padding(.vertical, Layout.rowVerticalPadding)
    }

    @ViewBuilder
    private var titleText: some View {
        let text = thread.title ?? thread.content
        if query.isEmpty {
            Text(text)
        } else {
            HighlightedText(text: text, query: query)
        }
    }

    @ViewBuilder
    private func subtitleText(_ text: String) -> some View {
        if query.isEmpty {
            Text(text)
        } else {
            HighlightedText(text: text, query: query)
        }
    }

    private var categoryBadge: some View {
        ZStack {
            RoundedRectangle(cornerRadius: Layout.badgeCornerRadius)
                .fill(thread.category.tint)
                .frame(width: Layout.badgeSize, height: Layout.badgeSize)
            Image(systemName: thread.category.icon)
                .font(.system(size: Layout.badgeIconSize, weight: .medium))
                .foregroundStyle(.white)
        }
    }
}
