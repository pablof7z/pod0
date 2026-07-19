import SwiftUI

struct RecallEvidenceCard: View {
    let evidence: RecallEvidence
    let open: () -> Void

    var body: some View {
        Button(action: open) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                HStack(alignment: .firstTextBaseline) {
                    VStack(alignment: .leading, spacing: 1) {
                        Text(evidence.episodeTitle)
                            .font(.subheadline.weight(.semibold))
                            .lineLimit(2)
                        Text(evidence.podcastTitle)
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
                Text("Transcript · \(provenanceLabel)")
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
        .accessibilityLabel("Play citation from \(evidence.episodeTitle) at \(timestamp)")
    }

    private var timestamp: String {
        PlayerTimeFormat.clock(Double(evidence.startMilliseconds) / 1_000)
    }

    private var provenanceLabel: String {
        switch evidence.provenance.source {
        case "publisher": "Publisher"
        case "scribe": "Scribe"
        case "whisper": "Whisper"
        case "onDevice": "On-device"
        case "assemblyAI": "AssemblyAI"
        default: "Transcript source"
        }
    }
}
