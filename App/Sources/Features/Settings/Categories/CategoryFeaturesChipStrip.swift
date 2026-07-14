import SwiftUI

// MARK: - CategoryFeaturesChipStrip
//
// Compact icon-only strip that telegraphs which features are enabled for
// a category. Used inside `CategoriesListView` rows so the user can scan
// the list and immediately see "this category has transcription off".
//
// Pure presentational — takes a snapshot of `CategorySettings` and renders.
// No bindings, no store access. The list view rebuilds the strip on every
// settings mutation through the normal SwiftUI invalidation path.

struct CategoryFeaturesChipStrip: View {
    let settings: CategorySettings

    var body: some View {
        HStack(spacing: 6) {
            chip(systemImage: "captions.bubble.fill",
                 enabled: settings.transcriptionEnabled,
                 enabledTint: .orange,
                 accessibility: "Transcription")
            chip(systemImage: "brain",
                 enabled: settings.ragEnabled,
                 enabledTint: .purple,
                 accessibility: "RAG indexing")
            chip(systemImage: "book.closed.fill",
                 enabled: settings.wikiGenerationEnabled,
                 enabledTint: .indigo,
                 accessibility: "Wiki generation")
            chip(systemImage: "sparkles.tv.fill",
                 enabled: settings.briefingsEnabled,
                 enabledTint: .pink,
                 accessibility: "Briefings")
            chip(systemImage: "bell.fill",
                 enabled: settings.notificationsEnabled,
                 enabledTint: .red,
                 accessibility: "Notifications")
            if settings.autoDownloadOverride != nil {
                chip(systemImage: "arrow.down.circle.fill",
                     enabled: true,
                     enabledTint: .blue,
                     accessibility: "Auto-download override")
            }
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
