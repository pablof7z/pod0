import SwiftUI

// MARK: - CategoriesListView
//
// Settings → Categories. Lists every category produced by the
// categorization service so the user can drill in and tweak the
// per-category feature toggles. Empty until the categorizer has run at
// least once, in which case we surface a friendly explainer instead of
// a blank screen.

struct CategoriesListView: View {
    @Environment(AppStateStore.self) private var store
    @State private var recomputeSheetPresented = false

    var body: some View {
        let projection = categoryProjection
        List {
            actionsSection
            if sortedCategories(projection: projection).isEmpty {
                emptyStateSection
            } else {
                categoriesSection(projection: projection)
            }
        }
        .settingsListStyle()
        .navigationTitle("Categories")
        .navigationBarTitleDisplayMode(.inline)
        .sheet(isPresented: $recomputeSheetPresented) {
            CategoriesRecomputeSheet()
        }
    }

    // MARK: - Sections

    private var actionsSection: some View {
        Section {
            Button {
                recomputeSheetPresented = true
            } label: {
                SettingsRow(
                    icon: "wand.and.sparkles",
                    tint: .green,
                    title: "Recompute Categories",
                    value: categoryCountLabel
                )
            }
            .buttonStyle(.plain)
            .foregroundStyle(.primary)
        } footer: {
            Text("Asks the configured model to regroup every podcast you follow. Existing categories are replaced.")
        }
    }

    private var emptyStateSection: some View {
        Section {
            VStack(spacing: AppTheme.Spacing.sm) {
                Image(systemName: "square.grid.2x2")
                    .font(.system(size: 32, weight: .semibold))
                    .foregroundStyle(.secondary)
                Text("No categories yet")
                    .font(AppTheme.Typography.headline)
                Text("Categories appear here once the agent has analysed your subscriptions.")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            .frame(maxWidth: .infinity)
            .padding(.vertical, AppTheme.Spacing.md)
        }
    }

    private func categoriesSection(projection: CategoryLibraryProjection) -> some View {
        let categories = sortedCategories(projection: projection)
        Section {
            ForEach(categories) { category in
                NavigationLink {
                    CategoryDetailView(categoryID: category.id)
                } label: {
                    row(for: category, projection: projection)
                }
            }
        } header: {
            Text("\(categories.count) categor\(categories.count == 1 ? "y" : "ies")")
        } footer: {
            Text("Tap a category to control transcription for every show inside. Other category-level policies will return once they are Rust-owned.")
        }
    }

    // MARK: - Row

    @ViewBuilder
    private func row(for category: PodcastCategory, projection: CategoryLibraryProjection) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(spacing: AppTheme.Spacing.sm) {
                Text(category.name.isEmpty ? category.slug : category.name)
                    .font(AppTheme.Typography.body)
                Spacer(minLength: 0)
                Text(subscriptionCountLabel(for: category, projection: projection))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            CategoryFeaturesChipStrip(
                transcriptionEnabled: projection.allTranscriptionEnabled(in: category.id) ?? true
            )
        }
        .padding(.vertical, 2)
    }

    // MARK: - Derived

    private var categoryProjection: CategoryLibraryProjection {
        CategoryLibraryProjection
            .load(categories: store.state.categories, store: store)
    }

    /// Categories sorted by Rust-owned category projection. Stable across
    /// re-renders so navigation pushes don't reshuffle siblings.
    private func sortedCategories(projection: CategoryLibraryProjection) -> [PodcastCategory] {
        projection
            .sortedCategories(from: store.state.categories)
    }

    private func subscriptionCountLabel(
        for category: PodcastCategory,
        projection: CategoryLibraryProjection
    ) -> String {
        let n = projection.podcastCount(in: category.id)
        return n == 1 ? "1 show" : "\(n) shows"
    }

    private var categoryCountLabel: String? {
        let count = store.state.categories.count
        guard count > 0 else { return nil }
        return "\(count)"
    }
}
