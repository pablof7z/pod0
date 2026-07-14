import SwiftUI

struct ElevenLabsVoiceRow: View {
    let voice: ElevenLabsVoice
    let isSelected: Bool
    let isPlaying: Bool
    let isLoadingPreview: Bool
    let canPreview: Bool
    let onTogglePreview: () -> Void

    // MARK: - Layout constants

    private enum Layout {
        /// Horizontal gap between the play button and the text stack.
        static let outerSpacing: CGFloat = 12
        /// Vertical gap between text elements inside the text stack.
        static let innerSpacing: CGFloat = 6
        /// Gap between capability pill badges.
        static let badgeSpacing: CGFloat = 6
        /// Minimum spacer length at the trailing edge.
        static let spacerMin: CGFloat = 4
        /// Row top/bottom padding.
        static let rowVPadding: CGFloat = 4
        /// Point size of the play/stop icon inside the button circle.
        static let playIconSize: CGFloat = 14
        /// Maximum number of pill labels to show per row.
        static let maxPillCount: Int = 4
    }

    var body: some View {
        HStack(alignment: .top, spacing: Layout.outerSpacing) {
            playButton

            VStack(alignment: .leading, spacing: Layout.innerSpacing) {
                HStack(alignment: .firstTextBaseline, spacing: Layout.innerSpacing) {
                    Text(voice.name)
                        .font(AppTheme.Typography.subheadline.weight(.semibold))
                        .foregroundStyle(.primary)
                        .lineLimit(2)

                    if isSelected {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(Color.accentColor)
                            .imageScale(.small)
                    }
                }

                Text(voice.voiceID)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                    .truncatedMiddle()

                if !voice.pillLabels.isEmpty {
                    HStack(spacing: Layout.badgeSpacing) {
                        ForEach(voice.pillLabels.prefix(Layout.maxPillCount), id: \.self) { pill in
                            ModelBadge(text: pill)
                        }
                    }
                }

                if let description = voice.descriptionLabel, !description.isEmpty {
                    Text(description.capitalized)
                        .font(AppTheme.Typography.caption2)
                        .foregroundStyle(.secondary)
                        .lineLimit(2)
                }
            }

            Spacer(minLength: Layout.spacerMin)
        }
        .padding(.vertical, Layout.rowVPadding)
        .contentShape(Rectangle())
    }

    private var playButton: some View {
        Button {
            onTogglePreview()
        } label: {
            ZStack {
                Circle()
                    .fill(buttonFill)
                    .frame(width: AppTheme.Layout.iconSm, height: AppTheme.Layout.iconSm)

                if isLoadingPreview {
                    ProgressView()
                        .controlSize(.small)
                        .tint(.white)
                } else {
                    Image(systemName: isPlaying ? "stop.fill" : "play.fill")
                        .font(.system(size: Layout.playIconSize, weight: .bold))
                        .foregroundStyle(.white)
                }
            }
        }
        .buttonStyle(.plain)
        .disabled(!canPreview)
        .opacity(canPreview ? 1 : 0.4)
        .accessibilityLabel(isPlaying ? "Stop preview" : "Play preview")
    }

    private var buttonFill: Color {
        guard canPreview else { return Color.secondary.opacity(0.4) }
        return isPlaying
            ? Color.red
            : AppTheme.Brand.elevenLabsTint
    }
}
