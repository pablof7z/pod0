import SwiftUI

// MARK: - KnowledgeSearchResultRow

/// One row in `KnowledgeSearchView`'s results list. Shows the podcast
/// name + episode title, the snippet excerpt (up to 3 lines), a
/// relevance bar, and a "seek to" button when the underlying chunk has
/// a timestamp.
///
/// Purely presentational — interaction surfaces via the `onSeek` closure
/// so the parent view owns the dispatch into `KernelModel`.
struct KnowledgeSearchResultRow: View {
    let result: KnowledgeSearchResult
    let onSeek: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            Text(result.podcastTitle)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
                .lineLimit(1)

            Text(result.episodeTitle)
                .font(AppTheme.Typography.headline)
                .lineLimit(2)

            Text(result.snippet)
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .lineLimit(3)
                .padding(.top, AppTheme.Spacing.xs)

            HStack(alignment: .center, spacing: AppTheme.Spacing.sm) {
                relevanceBar
                Spacer(minLength: AppTheme.Spacing.sm)
                if result.startSecs != nil {
                    seekButton
                }
            }
            .padding(.top, AppTheme.Spacing.xs)
        }
        .padding(.vertical, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .accessibilityElement(children: .combine)
        .accessibilityLabel(accessibilityLabel)
    }

    // MARK: - Relevance bar

    private var relevanceBar: some View {
        let fraction = max(0, min(1, result.relevanceScore))
        return GeometryReader { geo in
            ZStack(alignment: .leading) {
                Capsule()
                    .fill(Color.secondary.opacity(0.18))
                Capsule()
                    .fill(Color.accentColor)
                    .frame(width: geo.size.width * fraction)
            }
        }
        .frame(width: 80, height: 4)
        .accessibilityHidden(true)
    }

    // MARK: - Seek button

    private var seekButton: some View {
        Button(action: onSeek) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Image(systemName: "play.fill")
                if let secs = result.startSecs {
                    Text(formatDuration(secs)).font(AppTheme.Typography.monoCaption)
                }
            }
            .padding(.horizontal, AppTheme.Spacing.sm)
            .padding(.vertical, AppTheme.Spacing.xs)
            .background(Capsule().fill(Color.accentColor.opacity(0.15)))
            .foregroundStyle(Color.accentColor)
        }
        .buttonStyle(.borderless)
        .accessibilityLabel("Play from \(formatDuration(result.startSecs ?? 0))")
    }

    private var accessibilityLabel: String {
        var parts = [result.podcastTitle, result.episodeTitle, result.snippet]
        let pct = Int(max(0, min(1, result.relevanceScore)) * 100)
        parts.append("Relevance \(pct) percent")
        return parts.joined(separator: ", ")
    }
}
