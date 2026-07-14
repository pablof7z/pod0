import SwiftUI

// MARK: - ItemColorTag

/// A semantic color label the user can attach to an item for visual grouping.
///
/// Stored on `Item.colorTag` and rendered as a leading color stripe in
/// `HomeItemRow`. The `.none` case (the default) produces no visual stripe.
///
/// Colors resolve through the system adaptive palette so they look correct
/// in both light and dark mode.
enum ItemColorTag: String, Codable, Hashable, Sendable, CaseIterable {
    case none
    case red
    case orange
    case yellow
    case green
    case teal
    case blue
    case purple
    case pink

    // MARK: - Display

    /// Human-readable name shown in the color picker.
    var label: String {
        switch self {
        case .none:   return "None"
        case .red:    return "Red"
        case .orange: return "Orange"
        case .yellow: return "Yellow"
        case .green:  return "Green"
        case .teal:   return "Teal"
        case .blue:   return "Blue"
        case .purple: return "Purple"
        case .pink:   return "Pink"
        }
    }

    /// The resolved SwiftUI `Color` for UI rendering.
    ///
    /// All colors are semantic so they adapt to dark mode automatically.
    var color: Color {
        switch self {
        case .none:   return .clear
        case .red:    return .red
        case .orange: return .orange
        case .yellow: return Color(red: 0.95, green: 0.75, blue: 0.0)
        case .green:  return .green
        case .teal:   return .teal
        case .blue:   return .blue
        case .purple: return .purple
        case .pink:   return .pink
        }
    }

    /// SF Symbol for the "no color" option swatch in the picker.
    var systemImageName: String { "circle.slash" }
}
