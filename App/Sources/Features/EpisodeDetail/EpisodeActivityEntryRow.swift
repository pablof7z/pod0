import Pod0Core
import SwiftUI

struct EpisodeActivityEntryRow: View {
    let entry: EpisodeActivityEntry
    let isExpanded: Bool
    let onToggle: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 6) {
            Button(action: onToggle) {
                HStack(alignment: .top, spacing: 12) {
                    Image(systemName: iconName)
                        .font(.system(size: 16))
                        .foregroundStyle(tint)
                        .frame(width: 22)
                    VStack(alignment: .leading, spacing: 2) {
                        Text(entry.title)
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.primary)
                        Text(entry.summary)
                            .font(.caption)
                            .foregroundStyle(.secondary)
                            .multilineTextAlignment(.leading)
                    }
                    Spacer()
                    VStack(alignment: .trailing, spacing: 2) {
                        Text(timestamp, style: .time)
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                            .monospacedDigit()
                        Text(timestamp, format: .dateTime.month(.abbreviated).day())
                            .font(.caption2)
                            .foregroundStyle(.tertiary)
                    }
                }
            }
            .buttonStyle(.plain)
            if isExpanded { detailGrid.padding(.leading, 34) }
        }
        .padding(.vertical, 2)
    }

    private var detailGrid: some View {
        VStack(alignment: .leading, spacing: 4) {
            ForEach(Array(entry.details.enumerated()), id: \.offset) { _, detail in
                HStack(alignment: .top, spacing: 8) {
                    Text(detail.label)
                        .font(.caption2.weight(.medium))
                        .foregroundStyle(.secondary)
                        .frame(minWidth: 84, alignment: .leading)
                    Text(detail.value)
                        .font(.system(.caption2, design: .monospaced))
                        .foregroundStyle(.primary)
                        .textSelection(.enabled)
                        .frame(maxWidth: .infinity, alignment: .leading)
                }
            }
        }
        .padding(.vertical, 4)
    }

    private var timestamp: Date {
        Date(timeIntervalSince1970: Double(entry.committedAt.value) / 1_000)
    }

    private var tint: Color {
        switch entry.severity {
        case .info: .secondary
        case .success: AppTheme.Tint.success
        case .warning: AppTheme.Tint.warning
        case .failure: AppTheme.Tint.error
        }
    }

    private var iconName: String {
        switch entry.kind {
        case .request: "checkmark.circle"
        case .domainTransition: "arrow.triangle.branch"
        case .playbackCheckpoint: "play.circle"
        case .effectAuthorization: "bolt.badge.clock"
        case .effectObservation: "bolt.badge.checkmark"
        case .internalCommand: "gearshape.2"
        case .recovery: "arrow.clockwise"
        case .authorityCutover: "checkmark.shield"
        }
    }
}
