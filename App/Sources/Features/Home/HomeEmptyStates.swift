import SwiftUI

// MARK: - HomeFirstRunEmptyState

/// Pitch surface shown when the user has zero subscriptions. Lifted
/// verbatim from the legacy LibraryView so the onboarding moment carries
/// over unchanged.
struct HomeFirstRunEmptyState: View {
    let onAddShow: () -> Void

    var body: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Image(systemName: "books.vertical")
                .font(.system(size: 48, weight: .light))
                .foregroundStyle(.tertiary)
            VStack(spacing: AppTheme.Spacing.xs) {
                Text("Your shows live here.")
                    .font(AppTheme.Typography.title)
                Text("Search Apple Podcasts, paste a feed URL, or import an OPML file to begin.")
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            Button {
                Haptics.light()
                onAddShow()
            } label: {
                Label("Add Show", systemImage: "plus.circle.fill")
                    .padding(.horizontal, AppTheme.Spacing.md)
                    .padding(.vertical, AppTheme.Spacing.sm)
            }
            .buttonStyle(.glassProminent)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .frame(maxWidth: .infinity)
    }
}

// MARK: - HomeFilteredEmptyState

/// "No shows match this filter" state. Distinct from the first-run state
/// so a user with 40 subs and an active Transcribed filter doesn't see
/// the onboarding pitch as if their library disappeared.
///
/// When `categoryName` is set, the title/subtitle pair switches to a
/// category-aware copy ("No shows in Learning yet…") so the user knows
/// to either populate the section or flip back to All Categories.
struct HomeFilteredEmptyState: View {
    let filter: LibraryFilter
    var categoryName: String? = nil
    let onClearFilters: () -> Void

    var body: some View {
        VStack(spacing: AppTheme.Spacing.lg) {
            Image(systemName: glyph)
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.tertiary)
            VStack(spacing: AppTheme.Spacing.xs) {
                Text(title)
                    .font(AppTheme.Typography.title)
                    .multilineTextAlignment(.center)
                Text(subtitle)
                    .font(AppTheme.Typography.body)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            Button {
                Haptics.light()
                onClearFilters()
            } label: {
                Label("Clear filters", systemImage: "line.3.horizontal.decrease.circle")
                    .padding(.horizontal, AppTheme.Spacing.md)
                    .padding(.vertical, AppTheme.Spacing.sm)
            }
            .buttonStyle(.glass)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
        .frame(maxWidth: .infinity)
    }

    private var glyph: String {
        // When the category is set with the default `.all` library
        // filter, fall back to a folder-style glyph instead of the
        // library-filter-specific one.
        if categoryName != nil, filter == .all { return "tray" }
        return filter.emptyStateGlyph
    }

    private var title: String {
        if let categoryName, filter == .all {
            return "No shows in \(categoryName) yet."
        }
        return filter.emptyStateTitle
    }

    private var subtitle: String {
        if categoryName != nil, filter == .all {
            return "Add some, or switch categories."
        }
        if let categoryName {
            return "Nothing in \(categoryName) matches \(filter.label). Try clearing the filter."
        }
        return filter.emptyStateSubtitle
    }
}
