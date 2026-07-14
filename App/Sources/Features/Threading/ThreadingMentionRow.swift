import SwiftUI

// MARK: - Threading mention row

/// One row in `ThreadingTopicView`'s vertical timeline. Editorial typography,
/// an amber seam when the agent flagged the mention as contradictory, a
/// confidence dot in the top-right corner, and a leading-aligned timestamp
/// chip that fires the `play_episode` deep-link.
struct ThreadingMentionRow: View {

    let mention: ThreadingMention
    let episode: Episode?
    let subscriptionTitle: String?
    let onPlay: () -> Void

    private static let dateFormatter: DateFormatter = {
        let f = DateFormatter()
        f.dateStyle = .medium
        f.timeStyle = .none
        return f
    }()

    var body: some View {
        HStack(alignment: .top, spacing: 12) {
            seam
            VStack(alignment: .leading, spacing: 8) {
                topMeta
                if let title = episode?.title {
                    Text(title)
                        .font(AppTheme.Typography.callout.weight(.medium))
                        .foregroundStyle(.primary)
                        .lineLimit(2)
                }
                Text("\u{201C}\(mention.snippet)\u{201D}")
                    .font(AppTheme.Typography.caption)
                    .italic()
                    .foregroundStyle(.secondary)
                    .lineSpacing(2)
                bottomMeta
            }
        }
        .padding(.vertical, 6)
        .opacity(mention.confidence < 0.55 ? 0.7 : 1)
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    // MARK: - Subviews

    private var topMeta: some View {
        HStack(alignment: .firstTextBaseline, spacing: 8) {
            if let subscriptionTitle {
                Text(subscriptionTitle)
                    .font(.caption.weight(.semibold))
                    .foregroundStyle(.primary)
            }
            if let pub = episode?.pubDate {
                Text(ThreadingMentionRow.dateFormatter.string(from: pub))
                    .font(.caption)
                    .foregroundStyle(.tertiary)
                    .monospacedDigit()
            }
            Spacer(minLength: 0)
            confidenceDot
        }
    }

    private var bottomMeta: some View {
        HStack(spacing: 8) {
            timestampChip
            if mention.isContradictory {
                Label("contradicts", systemImage: "exclamationmark.triangle")
                    .labelStyle(.titleAndIcon)
                    .font(.caption2.weight(.medium))
                    .foregroundStyle(AppTheme.Tint.threadingContradiction)
            }
        }
    }

    private var seam: some View {
        Rectangle()
            .fill(mention.isContradictory
                ? AppTheme.Tint.threadingContradiction
                : Color.primary.opacity(0.12))
            .frame(width: 2)
            .frame(maxHeight: .infinity)
    }

    private var confidenceDot: some View {
        Circle()
            .fill(confidenceColor)
            .frame(width: 6, height: 6)
            .accessibilityHidden(true)
    }

    private var confidenceColor: Color {
        switch mention.confidence {
        case 0.75...: AppTheme.Tint.evidenceHigh
        case 0.5..<0.75: AppTheme.Tint.evidenceMedium
        default: AppTheme.Tint.evidenceLow
        }
    }

    private var timestampChip: some View {
        Button(action: onPlay) {
            HStack(spacing: 4) {
                Image(systemName: "play.fill")
                    .font(.caption2)
                Text(mention.formattedTimestamp)
                    .font(.system(.caption, design: .monospaced))
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 5)
            .background(
                Capsule()
                    .fill(Color.clear)
                    .glassEffect(.regular.interactive(), in: .capsule)
            )
            .overlay(
                Capsule()
                    .strokeBorder(ThreadingMentionRow.amber.opacity(0.35), lineWidth: 0.5)
            )
            .foregroundStyle(ThreadingMentionRow.amber)
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Play clip at \(mention.formattedTimestamp)")
    }

    private var accessibilityLabel: String {
        var parts: [String] = []
        if let subscriptionTitle { parts.append(subscriptionTitle) }
        if let title = episode?.title { parts.append(title) }
        parts.append(mention.snippet)
        if mention.isContradictory { parts.append("contradicts") }
        return parts.joined(separator: ", ")
    }

    /// Editorial amber, shared with `CitationChip` and wiki contradiction
    /// surfaces. Lives once in `AppTheme.Tint.editorialAmber`.
    static let amber = AppTheme.Tint.editorialAmber
}
