import SwiftUI

// MARK: - CategoryFeaturesChipStrip
//
// Compact icon-only strip for the category feature policy that is currently
// Rust-wired: transcription.
//
// Pure presentational — takes the Rust-projected category transcription state
// and renders. No bindings, no store access.

struct CategoryFeaturesChipStrip: View {
    let transcriptionEnabled: Bool

    var body: some View {
        HStack(spacing: 6) {
            chip(systemImage: "captions.bubble.fill",
                 enabled: transcriptionEnabled,
                 enabledTint: .orange,
                 accessibility: "Transcription")
        }
    }

    @ViewBuilder
    private func chip(
        systemImage: String,
        enabled: Bool,
        enabledTint: Color,
        accessibility: String
    ) -> some View {
        Image(systemName: systemImage)
            .font(.system(size: 11, weight: .semibold))
            .foregroundStyle(enabled ? enabledTint : Color.secondary.opacity(0.4))
            .accessibilityLabel("\(accessibility) \(enabled ? "on" : "off")")
    }
}
