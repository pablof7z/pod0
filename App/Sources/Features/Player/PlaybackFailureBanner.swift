import SwiftUI

struct PlaybackFailureBanner: View {
    let failure: UserFacingFailure
    let retry: () -> Void

    var body: some View {
        HStack(alignment: .top, spacing: AppTheme.Spacing.sm) {
            Image(systemName: failure.code == .offline ? "wifi.slash" : "exclamationmark.triangle")
                .font(.body.weight(.semibold))
                .foregroundStyle(.orange)
                .accessibilityHidden(true)

            VStack(alignment: .leading, spacing: 2) {
                Text(failure.title)
                    .font(.subheadline.weight(.semibold))
                Text(failure.message)
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            }
            .frame(maxWidth: .infinity, alignment: .leading)

            if failure.recoveryAction == .retry {
                Button("Try Again", action: retry)
                    .buttonStyle(.bordered)
                    .controlSize(.small)
            }
        }
        .padding(AppTheme.Spacing.sm)
        .background(.orange.opacity(0.08), in: RoundedRectangle(cornerRadius: AppTheme.Corner.md))
    }
}
