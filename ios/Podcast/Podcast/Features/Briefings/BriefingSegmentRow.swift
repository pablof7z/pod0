import SwiftUI

// MARK: - BriefingSegmentRow
//
// Card row rendering one `BriefingSegmentSummary` from the snapshot.
// The kind badge is derived from the snake_case `kind` string sent by
// Rust — switching here keeps the projection lean (no enum re-mapping
// on the wire) and centralises the human-readable label + icon.
//
// The view is intentionally dumb: it takes the projection row, renders
// it, and never touches the kernel directly. Tapping the card is a
// future hook (jump to player, open transcript) — wired in M9.B once
// the briefing player engine lands.

struct BriefingSegmentRow: View {
    let segment: BriefingSegmentSummary

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            header
            Text(segment.text)
                .font(.body)
                .foregroundStyle(.primary)
                .fixedSize(horizontal: false, vertical: true)
            if let attribution = attributionLine {
                Text(attribution)
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .lineLimit(2)
            }
        }
        .padding(AppTheme.Spacing.md)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .fill(.thinMaterial)
        )
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .stroke(Color.primary.opacity(0.06), lineWidth: 1)
        )
    }

    // MARK: - Header

    private var header: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: kindIcon)
                .font(.subheadline.weight(.semibold))
                .foregroundStyle(kindTint)
                .frame(width: 24, height: 24)
                .background(
                    Circle().fill(kindTint.opacity(0.15))
                )
            Text(kindLabel)
                .font(.caption.weight(.semibold))
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.5)
            Spacer()
        }
    }

    // MARK: - Attribution

    private var attributionLine: String? {
        switch (segment.podcastTitle, segment.episodeTitle) {
        case let (.some(podcast), .some(episode)):
            return "From \(podcast) — \(episode)"
        case let (.some(podcast), .none):
            return "From \(podcast)"
        case let (.none, .some(episode)):
            return episode
        case (.none, .none):
            return nil
        }
    }

    // MARK: - Kind metadata

    private var kindLabel: String {
        switch segment.kind {
        case "intro":                 "Intro"
        case "episode_summary":       "Episode Summary"
        case "new_episode_alert":     "New Episode"
        case "weather_update":        "Weather"
        case "outro_call_to_action":  "Outro"
        default:                      segment.kind.replacingOccurrences(of: "_", with: " ").capitalized
        }
    }

    private var kindIcon: String {
        switch segment.kind {
        case "intro":                 "sun.max.fill"
        case "episode_summary":       "headphones"
        case "new_episode_alert":     "bell.badge.fill"
        case "weather_update":        "cloud.sun.fill"
        case "outro_call_to_action":  "arrow.right.circle.fill"
        default:                      "text.bubble"
        }
    }

    private var kindTint: Color {
        switch segment.kind {
        case "intro":                 .orange
        case "episode_summary":       .accentColor
        case "new_episode_alert":     .pink
        case "weather_update":        .blue
        case "outro_call_to_action":  .green
        default:                      .secondary
        }
    }
}
