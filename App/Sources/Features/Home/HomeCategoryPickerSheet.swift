import SwiftUI

// MARK: - HomeCategoryPickerSheet
//
// Sheet opened from the Home navigation title. Shows all available
// categories as rich HomeCategoryCards (artwork strip + LLM description +
// show/episode stats) plus a top "Home" row that resets to the global
// all-categories view.
//
// Selection forwards through `onSelect`; the sheet self-dismisses after
// each pick. The caller owns persistence (via @AppStorage on categoryFilterID).

struct HomeCategoryPickerSheet: View {
    @Environment(AppStateStore.self) private var store
    @Environment(\.dismiss) private var dismiss

    let selectedCategoryID: UUID?
    let onSelect: (UUID?) -> Void

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 0) {
                    homeSection
                    if !sortedCategories.isEmpty {
                        categoriesSection
                    }
                }
                .padding(AppTheme.Spacing.md)
                .padding(.bottom, AppTheme.Spacing.xl)
            }
            .navigationTitle("Browse")
            .navigationBarTitleDisplayMode(.inline)
            .navigationDestination(for: UUID.self) { categoryID in
                CategoryDetailView(categoryID: categoryID)
            }
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button("Done") {
                        Haptics.light()
                        dismiss()
                    }
                }
            }
            .background(Color(.systemGroupedBackground).ignoresSafeArea())
        }
    }

    // MARK: - Sections

    private var homeSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            homeRow
        }
    }

    private var categoriesSection: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.sm) {
            sectionLabel("Categories")
                .padding(.top, AppTheme.Spacing.md)
            ForEach(sortedCategories) { category in
                categoryCard(category)
            }
        }
    }

    // MARK: - Home row

    private var homeRow: some View {
        let isActive = selectedCategoryID == nil
        return Button {
            Haptics.light()
            onSelect(nil)
            dismiss()
        } label: {
            HStack(spacing: AppTheme.Spacing.md) {
                homeIcon
                homeLabels
                Spacer(minLength: 0)
                if isActive {
                    checkmark
                }
            }
            .padding(AppTheme.Spacing.md)
            .background(rowBackground(isActive: isActive))
        }
        .buttonStyle(.plain)
        .animation(.easeInOut(duration: 0.2), value: isActive)
    }

    private var homeIcon: some View {
        ZStack {
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color.accentColor.opacity(0.15))
                .frame(width: 44, height: 44)
            Image(systemName: "house.fill")
                .font(.system(size: 18, weight: .medium))
                .foregroundStyle(Color.accentColor)
        }
    }

    private var homeLabels: some View {
        VStack(alignment: .leading, spacing: 2) {
            Text("Home")
                .font(AppTheme.Typography.subheadline.weight(.semibold))
                .foregroundStyle(.primary)
            Text("All categories · \(store.state.subscriptions.count) shows")
                .font(AppTheme.Typography.caption)
                .foregroundStyle(.secondary)
        }
    }

    private var checkmark: some View {
        Image(systemName: "checkmark.circle.fill")
            .font(.system(size: 18, weight: .medium))
            .foregroundStyle(Color.accentColor)
    }

    private func rowBackground(isActive: Bool) -> some View {
        RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
            .fill(Color(.secondarySystemGroupedBackground))
            .overlay(
                RoundedRectangle(cornerRadius: AppTheme.Corner.lg, style: .continuous)
                    .strokeBorder(
                        isActive ? Color.accentColor.opacity(0.4) : Color.clear,
                        lineWidth: 1.5
                    )
            )
    }

    // MARK: - Category card
    //
    // ZStack overlays a gear NavigationLink at the top-trailing corner.
    // SwiftUI gives the deepest interactive control gesture priority, so
    // tapping the gear navigates to CategoryDetailView (within the same
    // NavigationStack) while tapping elsewhere on the card selects the
    // category and dismisses the sheet.

    private func categoryCard(_ category: PodcastCategory) -> some View {
        ZStack(alignment: .topTrailing) {
            HomeCategoryCard(
                category: category,
                subscriptions: subscriptions(in: category),
                unplayedTotal: unplayedTotal(for: category),
                isSelected: selectedCategoryID == category.id,
                onTap: {
                    Haptics.light()
                    onSelect(category.id)
                    dismiss()
                }
            )
            NavigationLink(value: category.id) {
                Image(systemName: "gearshape")
                    .font(.system(size: 13, weight: .medium))
                    .foregroundStyle(.secondary.opacity(0.7))
                    .padding(AppTheme.Spacing.md)
                    .contentShape(Rectangle())
            }
        }
    }

    // MARK: - Helpers

    private func sectionLabel(_ text: String) -> some View {
        Text(text.uppercased())
            .font(AppTheme.Typography.caption2.weight(.semibold))
            .foregroundStyle(.secondary)
            .tracking(0.5)
            .padding(.horizontal, 4)
    }

    private var sortedCategories: [PodcastCategory] {
        store.state.categories.sorted {
            $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending
        }
    }

    private func subscriptions(in category: PodcastCategory) -> [Podcast] {
        category.subscriptionIDs.compactMap { store.podcast(id: $0) }
    }

    private func unplayedTotal(for category: PodcastCategory) -> Int {
        category.subscriptionIDs.reduce(0) { $0 + store.unplayedCount(forPodcast: $1) }
    }
}
