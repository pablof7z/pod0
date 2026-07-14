import SwiftUI

// MARK: - ClipSubtitleStyle

/// Subtitle style toggle from UX-03 §6.6. The composer offers Editorial
/// (serif body, calm) and Bold (rounded display weight, social-first).
enum ClipSubtitleStyle: String, CaseIterable, Identifiable, Sendable {
    case editorial
    case bold

    var id: String { rawValue }

    var label: String {
        switch self {
        case .editorial: return "Editorial"
        case .bold:      return "Bold"
        }
    }
}

// MARK: - ClipPreviewView

/// Card-style preview the composer renders above its controls. Mirrors the
/// quote-share card visually but reads from a draft `Clip` shape so the
/// composer can update the preview live as the user drags handles, edits
/// the caption, or flips the style toggle.
struct ClipPreviewView: View {

    let transcriptText: String
    let speakerLabel: String?
    let timestampLabel: String
    let caption: String?
    let style: ClipSubtitleStyle
    let showSpeakerLabel: Bool

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.md) {
            if let caption, !caption.isEmpty {
                Text(caption)
                    .font(.system(.caption, design: .rounded).weight(.semibold))
                    .tracking(0.4)
                    .foregroundStyle(.secondary)
                    .textCase(.uppercase)
            }

            Text("\u{201C}\(transcriptText)\u{201D}")
                .font(prose)
                .foregroundStyle(.primary)
                .lineSpacing(style == .editorial ? 6 : 4)
                .fixedSize(horizontal: false, vertical: true)

            HStack(spacing: 8) {
                if showSpeakerLabel, let speakerLabel {
                    Text("\u{2014} \(speakerLabel)")
                        .font(.system(.footnote, design: .rounded).weight(.semibold))
                        .foregroundStyle(.primary)
                }
                Text(timestampLabel)
                    .font(.system(.caption2, design: .monospaced))
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
                Spacer()
            }
        }
        .padding(AppTheme.Spacing.lg)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .fill(Color(.systemBackground))
        )
        .overlay(
            RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                .strokeBorder(Color.secondary.opacity(0.18), lineWidth: 0.5)
        )
        .shadow(color: Color.black.opacity(0.06), radius: 18, y: 6)
    }

    private var prose: Font {
        switch style {
        case .editorial:
            return AppTheme.Typography.body
        case .bold:
            return .system(size: 20, weight: .bold, design: .rounded)
        }
    }
}

// MARK: - Preview

#Preview {
    VStack(spacing: AppTheme.Spacing.lg) {
        ClipPreviewView(
            transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
            speakerLabel: "Peter Attia",
            timestampLabel: "14:31 \u{2192} 14:48",
            caption: "On metabolism",
            style: .editorial,
            showSpeakerLabel: true
        )
        ClipPreviewView(
            transcriptText: "Metabolic flexibility isn't a diet — it's a property of the mitochondria.",
            speakerLabel: "Peter Attia",
            timestampLabel: "14:31 \u{2192} 14:48",
            caption: nil,
            style: .bold,
            showSpeakerLabel: false
        )
    }
    .padding()
    .background(Color(.systemGroupedBackground))
}
