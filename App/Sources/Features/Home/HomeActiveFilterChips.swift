import SwiftUI

// MARK: - HomeActiveFilterChip

/// One dismissible chip surfaced in the Home active-filter strip.
struct HomeActiveFilterChip: Identifiable, Equatable, Sendable {

    enum Kind: Equatable, Sendable {
        case libraryFilter(LibraryFilter)
        case category(UUID, String)
    }

    let kind: Kind
    let label: String

    var id: String {
        switch kind {
        case .libraryFilter(let f): return "filter:\(f.rawValue)"
        case .category(let id, _):  return "category:\(id.uuidString)"
        }
    }
}

// MARK: - Pure derivation

enum HomeActiveFilters {

    /// Build the chip list from the current filter selections. Pure function
    /// — `categoryName` resolves the category id to a display name (the
    /// store's `category(id:)` lookup is what the caller passes in). The
    /// `.all` library filter is treated as the unfiltered default and never
    /// surfaces a chip.
    static func chips(
        filter: LibraryFilter,
        categoryID: UUID?,
        categoryName: (UUID) -> String?
    ) -> [HomeActiveFilterChip] {
        var chips: [HomeActiveFilterChip] = []
        if filter != .all {
            chips.append(HomeActiveFilterChip(
                kind: .libraryFilter(filter),
                label: filter.label
            ))
        }
        if let id = categoryID, let name = categoryName(id) {
            chips.append(HomeActiveFilterChip(
                kind: .category(id, name),
                label: name
            ))
        }
        return chips
    }
}

// MARK: - View

/// Horizontal strip of active-filter chips. Returns `EmptyView` when no
/// chips are active so callers can drop the row entirely without a stray
/// padded blank rectangle in the layout.
struct HomeActiveFilterChipStrip: View {
    let chips: [HomeActiveFilterChip]
    let onDismiss: (HomeActiveFilterChip) -> Void

    var body: some View {
        if chips.isEmpty {
            EmptyView()
        } else {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.sm) {
                    ForEach(chips) { chip in
                        HomeActiveFilterChipView(
                            chip: chip,
                            onDismiss: { onDismiss(chip) }
                        )
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
            }
        }
    }
}

private struct HomeActiveFilterChipView: View {
    let chip: HomeActiveFilterChip
    let onDismiss: () -> Void

    var body: some View {
        Button {
            Haptics.light()
            onDismiss()
        } label: {
            HStack(spacing: AppTheme.Spacing.xs) {
                Text(chip.label)
                    .font(AppTheme.Typography.caption)
                Image(systemName: "xmark")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .foregroundStyle(.primary)
            .background(
                Capsule(style: .continuous)
                    .fill(Color(.tertiarySystemFill))
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(chip.label) filter active")
        // Hints describe the effect, not the gesture — VoiceOver
        // narrates "double-tap to activate" via the button trait
        // automatically, so the hint shouldn't repeat that.
        .accessibilityHint("Clears this filter")
    }
}
