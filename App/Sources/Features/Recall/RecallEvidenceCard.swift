import SwiftUI
import Pod0Core

struct RecallEvidenceCard: View {
    @Environment(AppStateStore.self) private var store
    let evidence: RecallEvidenceProjection
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(episodeTitle)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(2)
                        Text(podcastTitle)
                            .font(AppTheme.Typography.caption)
                            .foregroundStyle(.secondary)
                            .lineLimit(1)
                    }
                    Spacer(minLength: AppTheme.Spacing.sm)
                    Label(timestamp, systemImage: "play.fill")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.tint)
                }
                Text(evidence.excerpt)
                    .font(AppTheme.Typography.callout)
                    .foregroundStyle(.primary)
                    .lineLimit(5)
                    .multilineTextAlignment(.leading)
                Text("Transcript evidence")
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.tertiary)
            }
            .padding(AppTheme.Spacing.sm)
            .frame(maxWidth: .infinity, alignment: .leading)
            .glassEffect(
                .regular.tint(AppTheme.Tint.agentSurface.opacity(0.08)).interactive(),
                in: .rect(cornerRadius: AppTheme.Corner.md)
            )
        }
        .buttonStyle(.plain)
        .accessibilityLabel("Play citation from \(episodeTitle) at \(timestamp)")
    }

    private var timestamp: String {
        PlayerTimeFormat.clock(Double(evidence.startMilliseconds) / 1_000)
    }

    private var episodeTitle: String {
        guard let id = evidence.episodeId.uuid else { return "Episode" }
        return store.episode(id: id)?.title ?? "Episode"
    }

    private var podcastTitle: String {
        guard let id = evidence.podcastId.uuid else { return "Podcast" }
        return store.podcast(id: id)?.title ?? "Podcast"
    }
}
