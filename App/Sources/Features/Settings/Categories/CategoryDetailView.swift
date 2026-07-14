import SwiftUI

// MARK: - CategoryDetailView
//
// Settings → Categories → <one category>. Form with the full set of
// per-category knobs:
//   • Auto-download override (off when the toggle is off; falls back to
//     the per-subscription policy as it stands today).
//   • Per-feature toggles: transcription, RAG, wiki, briefings, notifs.
//   • Read-only list of subscriptions in this category.
//
// The auto-download picker mirrors `ShowDetailSettingsSheet`'s 3-way
// segmented control + Wi-Fi toggle + latest-N stepper, gated behind an
// "Override app default" toggle so the spec's `nil = inherit` semantic
// is preserved without re-implementing a custom 4-way picker.

struct CategoryDetailView: View {
    @Environment(AppStateStore.self) private var store
    let categoryID: UUID

    var body: some View {
        Form {
            if let category = store.state.categories.first(where: { $0.id == categoryID }) {
                if !category.description.isEmpty {
                    descriptionSection(category)
                }
                autoDownloadSection
                featuresSection
                subscriptionsSection(category)
            } else {
                missingCategorySection
            }
        }
        .navigationTitle(navigationTitle)
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sections

    private func descriptionSection(_ category: PodcastCategory) -> some View {
        Section {
            Text(category.description)
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var autoDownloadSection: some View {
        CategoryAutoDownloadSection(
            settings: store.categorySettings(for: categoryID),
            onUpdate: { block in
                store.updateCategorySettings(categoryID, block)
            }
        )
    }

    private var featuresSection: some View {
        Section {
            Toggle(isOn: toggleBinding(\.transcriptionEnabled)) {
                Label("Transcription", systemImage: "captions.bubble.fill")
            }
            Toggle(isOn: toggleBinding(\.ragEnabled)) {
                Label("RAG indexing", systemImage: "brain")
            }
            Toggle(isOn: toggleBinding(\.wikiGenerationEnabled)) {
                Label("Wiki generation", systemImage: "book.closed.fill")
            }
            Toggle(isOn: toggleBinding(\.briefingsEnabled)) {
                Label("Briefings", systemImage: "sparkles.tv.fill")
            }
            Toggle(isOn: toggleBinding(\.notificationsEnabled)) {
                Label("Notifications", systemImage: "bell.fill")
            }
        } header: {
            Text("Features")
        } footer: {
            Text("Disable features for categories you don't want analysed — entertainment shows, for example, often don't need transcripts or wiki entries.")
        }
    }

    @ViewBuilder
    private func subscriptionsSection(_ category: PodcastCategory) -> some View {
        let pods = podcasts(in: category)
        Section {
            if pods.isEmpty {
                Text("No subscriptions assigned yet.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
            } else {
                ForEach(pods) { pod in
                    HStack(spacing: AppTheme.Spacing.sm) {
                        Text(pod.title.isEmpty ? (pod.feedURL?.host ?? "Untitled") : pod.title)
                            .font(AppTheme.Typography.body)
                            .lineLimit(1)
                        Spacer(minLength: 0)
                        if !pod.author.isEmpty {
                            Text(pod.author)
                                .font(AppTheme.Typography.caption2)
                                .foregroundStyle(.secondary)
                                .lineLimit(1)
                        }
                    }
                }
            }
        } header: {
            Text(pods.count == 1 ? "1 show" : "\(pods.count) shows")
        }
    }

    private var missingCategorySection: some View {
        Section {
            Text("This category is no longer available. It may have been removed by the categorization service.")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    // MARK: - Bindings

    private func toggleBinding(_ keyPath: WritableKeyPath<CategorySettings, Bool>) -> Binding<Bool> {
        Binding(
            get: { store.categorySettings(for: categoryID)[keyPath: keyPath] },
            set: { newValue in
                store.updateCategorySettings(categoryID) { settings in
                    settings[keyPath: keyPath] = newValue
                }
                Haptics.selection()
            }
        )
    }

    // MARK: - Derived

    private var navigationTitle: String {
        store.state.categories.first(where: { $0.id == categoryID })?.name ?? "Category"
    }

    /// Resolves a category's podcast IDs to live `Podcast` rows, dropping
    /// entries the user may have unsubscribed from since the categorizer
    /// last ran. Sorted by title to match every other management surface.
    private func podcasts(in category: PodcastCategory) -> [Podcast] {
        category.subscriptionIDs
            .compactMap { store.podcast(id: $0) }
            .sorted { $0.title.localizedCaseInsensitiveCompare($1.title) == .orderedAscending }
    }
}
