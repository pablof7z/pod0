import SwiftUI

// MARK: - Layout

private enum Layout {
    /// Vertical spacing between elements in all state views.
    static let stateSpacing: CGFloat = 16
    /// Icon size for the error state view.
    static let errorIconSize: CGFloat = 36
    /// Icon size for the missing-key state view.
    static let missingKeyIconSize: CGFloat = 44
}

// MARK: - ElevenLabsVoiceGroup

struct ElevenLabsVoiceGroup: Hashable {
    let category: String
    let voices: [ElevenLabsVoice]
}

// MARK: - State views

/// Full-screen loading placeholder shown while voices are being fetched.
struct ElevenLabsVoiceBrowserLoadingView: View {
    var body: some View {
        VStack(spacing: Layout.stateSpacing) {
            ProgressView()
                .controlSize(.large)
            Text("Loading voices")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}

/// Full-screen error view with a retry button.
struct ElevenLabsVoiceBrowserErrorView: View {
    let message: String
    let onRetry: () -> Void

    var body: some View {
        VStack(spacing: Layout.stateSpacing) {
            Image(systemName: "exclamationmark.triangle")
                .font(.system(size: Layout.errorIconSize))
                .foregroundStyle(AppTheme.Tint.warning)
                .accessibilityHidden(true)
            Text(message)
                .font(AppTheme.Typography.subheadline)
                .multilineTextAlignment(.center)
                .foregroundStyle(.secondary)
                .padding(.horizontal, AppTheme.Spacing.lg)
            Button(action: onRetry) {
                Label("Try again", systemImage: "arrow.clockwise")
            }
            .buttonStyle(.glassProminent)
            .tint(AppTheme.Brand.elevenLabsTint)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}

/// Full-screen placeholder shown when no ElevenLabs API key is stored.
struct ElevenLabsVoiceBrowserMissingKeyView: View {
    let onBack: () -> Void

    var body: some View {
        VStack(spacing: Layout.stateSpacing) {
            Image(systemName: "waveform.slash")
                .font(.system(size: Layout.missingKeyIconSize))
                .foregroundStyle(.secondary)
                .accessibilityHidden(true)
            Text("Connect ElevenLabs to browse voices")
                .font(AppTheme.Typography.headline)
                .multilineTextAlignment(.center)
            Text("Add your ElevenLabs API key in the previous screen to load the voice library.")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
                .padding(.horizontal, AppTheme.Spacing.lg)
            Button(action: onBack) {
                Label("Back to ElevenLabs Settings", systemImage: "chevron.backward")
            }
            .buttonStyle(.glassProminent)
            .tint(AppTheme.Brand.elevenLabsTint)
        }
        .frame(maxWidth: .infinity, maxHeight: .infinity)
        .background(Color(.systemGroupedBackground))
    }
}
