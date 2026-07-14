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
        List {
            actionsSection
            if sortedCategories.isEmpty {
                emptyStateSection
            } else {
                categoriesSection
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

    private var categoriesSection: some View {
        Section {
            ForEach(sortedCategories) { category in
                NavigationLink {
                    CategoryDetailView(categoryID: category.id)
                } label: {
                    row(for: category)
                }
            }
        } header: {
            Text("\(sortedCategories.count) categor\(sortedCategories.count == 1 ? "y" : "ies")")
        } footer: {
            Text("Tap a category to override auto-download, transcription, RAG, wiki, briefings, and notifications for every show inside.")
        }
    }

    // MARK: - Row

    @ViewBuilder
    private func row(for category: PodcastCategory) -> some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
            HStack(spacing: AppTheme.Spacing.sm) {
                Text(category.name.isEmpty ? category.slug : category.name)
                    .font(AppTheme.Typography.body)
                Spacer(minLength: 0)
                Text(subscriptionCountLabel(for: category))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
                    .monospacedDigit()
            }
            CategoryFeaturesChipStrip(settings: store.categorySettings(for: category.id))
        }
        .padding(.vertical, 2)
    }

    // MARK: - Derived

    /// Categories sorted by display name (case-insensitive). Stable across
    /// re-renders so navigation pushes don't reshuffle siblings.
    private var sortedCategories: [PodcastCategory] {
        store.state.categories.sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
    }

    private func subscriptionCountLabel(for category: PodcastCategory) -> String {
        let n = category.subscriptionIDs.count
        return n == 1 ? "1 show" : "\(n) shows"
    }

    private var categoryCountLabel: String? {
        let count = store.state.categories.count
        guard count > 0 else { return nil }
        return "\(count)"
    }
}
