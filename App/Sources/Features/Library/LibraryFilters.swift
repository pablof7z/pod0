import SwiftUI

// MARK: - LibraryFilter

/// Filter applied to the subscriptions grid in the Library tab.
///
/// These operate on **structured fields** (status, downloaded, transcript
/// availability) — they are *not* a search. The "ask anything" semantic
/// search lives in the Ask tab and is reached via the search-entry bar
/// at the top of `LibraryView`.
///
/// Lane 2 will replace the closure-based predicate driving these chips
/// with real subscription queries; the enum itself can stay.
enum LibraryFilter: String, CaseIterable, Identifiable, Hashable {
    case all
    case unplayed
    case downloaded
    case transcribed

    var id: String { rawValue }

    /// User-visible chip label. Kept short so the rail fits on iPhone SE.
    var label: String {
        switch self {
        case .all:          return "All"
        case .unplayed:     return "Unplayed"
        case .downloaded:   return "Downloaded"
        case .transcribed:  return "Transcribed"
        }
    }

    /// SF Symbol for the chip glyph. The glyph is hidden on the
    /// `.all` chip — "All" needs no decoration.
    var systemImage: String? {
        switch self {
        case .all:          return nil
        case .unplayed:     return "circle.fill"
        case .downloaded:   return "arrow.down.circle.fill"
        case .transcribed:  return "text.bubble.fill"
        }
    }

    /// Glyph for the "no shows match this filter" empty state. `.all` should
    /// never reach the filtered-empty branch — its empty state is the
    /// genuine fresh-user pitch — but a fallback keeps the property total.
    var emptyStateGlyph: String {
        switch self {
        case .all:          return "books.vertical"
        case .unplayed:     return "circle.dashed"
        case .downloaded:   return "arrow.down.circle"
        case .transcribed:  return "text.bubble"
        }
    }

    /// Title for the filtered-empty state. Naming the filter by name avoids
    /// the "Your shows live here" fresh-user copy showing up to a user with
    /// 40+ subscriptions whose Transcribed filter happens to match nothing.
    var emptyStateTitle: String {
        switch self {
        case .all:          return "Your shows live here."
        case .unplayed:     return "Nothing unplayed."
        case .downloaded:   return "No downloaded shows."
        case .transcribed:  return "No transcribed shows yet."
        }
    }

    /// Subtitle that explains why the filter is empty and hints at what
    /// the user can do — distinct from the first-run copy that pitches
    /// adding a first show.
    var emptyStateSubtitle: String {
        switch self {
        case .all:
            return "Search Apple Podcasts, paste a feed URL, or import an OPML file to begin."
        case .unplayed:
            return "Every subscribed show has been listened through. Tap Show all to see your library."
        case .downloaded:
            return "No episodes are downloaded for offline listening yet. Download from any episode row."
        case .transcribed:
            return "Connect ElevenLabs in Settings and request a transcript on any episode to populate this filter."
        }
    }
}

// MARK: - LibraryFilterChip

/// A single matte chip in the filter rail. Rendered as a capsule with
/// secondary background when inactive and the app accent tint when active.
///
/// Glass is reserved for the **rail container** (the parent), not for
/// individual chips — per the lane brief, cards-and-children are matte.
struct LibraryFilterChip: View {
    let filter: LibraryFilter
    let isActive: Bool
    let action: () -> Void

    var body: some View {
        Button(action: tap) {
            HStack(spacing: AppTheme.Spacing.xs) {
                if let symbol = filter.systemImage {
                    Image(systemName: symbol)
                        .font(.caption2.weight(.semibold))
                }
                Text(filter.label)
                    .font(AppTheme.Typography.caption)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .foregroundStyle(isActive ? Color.white : Color.primary)
            .background(
                Capsule(style: .continuous)
                    .fill(isActive
                          ? AnyShapeStyle(Color.accentColor)
                          : AnyShapeStyle(Color(.tertiarySystemFill)))
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel(filter.label)
        .accessibilityAddTraits(isActive ? .isSelected : [])
    }

    private func tap() {
        Haptics.selection()
        action()
    }
}

// MARK: - LibraryFilterRail

/// Horizontal scrolling rail of filter chips. The rail itself sits inside
/// a `glassSurface` container in `LibraryView` (structural glass) — this
/// view only owns the chip layout and selection bridge.
struct LibraryFilterRail: View {
    @Binding var selection: LibraryFilter

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: AppTheme.Spacing.sm) {
                ForEach(LibraryFilter.allCases) { filter in
                    LibraryFilterChip(
                        filter: filter,
                        isActive: selection == filter,
                        action: { withAnimation(AppTheme.Animation.springFast) {
                            selection = filter
                        } }
                    )
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
        }
    }
}

// MARK: - LibraryCategoryRail

struct LibraryCategoryRail: View {
    let categories: [PodcastCategory]
    let counts: [UUID: Int]
    @Binding var selection: UUID?

    private var totalCount: Int {
        counts.values.reduce(0, +)
    }

    var body: some View {
        ScrollView(.horizontal, showsIndicators: false) {
            HStack(spacing: AppTheme.Spacing.sm) {
                LibraryCategoryChip(
                    title: "All Categories",
                    count: totalCount,
                    color: .accentColor,
                    isActive: selection == nil,
                    action: { select(nil) }
                )

                ForEach(categories) { category in
                    LibraryCategoryChip(
                        title: category.name,
                        count: counts[category.id] ?? 0,
                        color: category.displayColor,
                        isActive: selection == category.id,
                        action: { select(category.id) }
                    )
                }
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
        }
    }

    private func select(_ categoryID: UUID?) {
        Haptics.selection()
        withAnimation(AppTheme.Animation.springFast) {
            selection = categoryID
        }
    }
}

private struct LibraryCategoryChip: View {
    let title: String
    let count: Int
    let color: Color
    let isActive: Bool
    let action: () -> Void

    var body: some View {
        Button(action: action) {
            HStack(spacing: AppTheme.Spacing.xs) {
                Circle()
                    .fill(isActive ? Color.white : color)
                    .frame(width: 8, height: 8)
                Text(title)
                    .font(AppTheme.Typography.caption)
                    .lineLimit(1)
                Text("\(count)")
                    .font(.caption2.weight(.semibold))
                    .monospacedDigit()
                    .foregroundStyle(isActive ? Color.white.opacity(0.78) : Color.secondary)
            }
            .padding(.horizontal, AppTheme.Spacing.md)
            .padding(.vertical, AppTheme.Spacing.sm)
            .foregroundStyle(isActive ? Color.white : Color.primary)
            .background(
                Capsule(style: .continuous)
                    .fill(isActive
                          ? AnyShapeStyle(color)
                          : AnyShapeStyle(Color(.tertiarySystemFill)))
            )
            .contentShape(Capsule())
        }
        .buttonStyle(.plain)
        .accessibilityLabel("\(title), \(count) shows")
        .accessibilityAddTraits(isActive ? .isSelected : [])
    }
}

private extension PodcastCategory {
    var displayColor: Color {
        Color(categoryHex: colorHex) ?? .accentColor
    }
}

private extension Color {
    init?(categoryHex rawValue: String?) {
        guard var hex = rawValue?.trimmingCharacters(in: .whitespacesAndNewlines),
              !hex.isEmpty
        else { return nil }
        if hex.hasPrefix("#") {
            hex.removeFirst()
        }
        guard hex.count == 6 || hex.count == 8,
              let value = UInt64(hex, radix: 16)
        else { return nil }
        let r: Double
        let g: Double
        let b: Double
        if hex.count == 8 {
            r = Double((value >> 24) & 0xFF) / 255.0
            g = Double((value >> 16) & 0xFF) / 255.0
            b = Double((value >> 8) & 0xFF) / 255.0
        } else {
            r = Double((value >> 16) & 0xFF) / 255.0
            g = Double((value >> 8) & 0xFF) / 255.0
            b = Double(value & 0xFF) / 255.0
        }
        self.init(red: r, green: g, blue: b)
    }
}
