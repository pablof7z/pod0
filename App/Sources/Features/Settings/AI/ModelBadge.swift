import SwiftUI

// MARK: - ModelBadgeKind

/// Typed capability badges for OpenRouter model rows.
///
/// Using a typed enum instead of raw strings ensures that `ModelBadge` and
/// `OpenRouterModelRow` always agree on both the display label and its color —
/// a change to one can never silently break the other.
enum ModelBadgeKind: Hashable {
    /// Model does not guarantee JSON-compatible output.
    case noJSON
    /// Model supports function/tool calling.
    case tools
    /// Model has a dedicated reasoning mode.
    case reasoning
    /// Model accepts image inputs.
    case vision
    /// Model weights are publicly available.
    case openWeights
    /// Model is available at no cost.
    case free

    /// Human-readable label shown inside the badge capsule.
    var text: String {
        switch self {
        case .noJSON:      return "No JSON"
        case .tools:       return "Tools"
        case .reasoning:   return "Reasoning"
        case .vision:      return "Vision"
        case .openWeights: return "Open"
        case .free:        return "Free"
        }
    }

    /// Accent color for the badge text.
    var color: Color {
        switch self {
        case .noJSON:      return .orange
        case .tools:       return .purple
        case .reasoning:   return .indigo
        case .vision:      return .teal
        case .openWeights: return .green
        case .free:        return .secondary
        }
    }
}

// MARK: - ModelBadge

/// Compact capsule chip used on model rows to indicate capabilities or metadata.
///
/// Use `ModelBadge(kind:)` for typed OpenRouter capability badges — label and
/// color are derived from `ModelBadgeKind` and stay in sync automatically.
/// Use `ModelBadge(text:)` when surfacing arbitrary metadata strings (e.g.
/// ElevenLabs voice labels) where no semantic color mapping exists.
struct ModelBadge: View {
    var text: String
    var color: Color

    // MARK: - Layout constants

    private enum Layout {
        /// Horizontal padding inside the badge capsule.
        static let paddingH: CGFloat = 7
        /// Vertical padding inside the badge capsule.
        static let paddingV: CGFloat = 3
    }

    /// Typed initializer for OpenRouter model capability badges.
    init(kind: ModelBadgeKind) {
        self.text = kind.text
        self.color = kind.color
    }

    /// Generic initializer for arbitrary label strings (e.g. voice metadata).
    init(text: String) {
        self.text = text
        self.color = .secondary
    }

    var body: some View {
        Text(text)
            .font(AppTheme.Typography.caption2.weight(.medium))
            .foregroundStyle(color)
            .lineLimit(1)
            .padding(.horizontal, Layout.paddingH)
            .padding(.vertical, Layout.paddingV)
            .background(
                Capsule(style: .continuous)
                    .fill(Color(.tertiarySystemFill))
            )
    }
}

// MARK: - Preview

#Preview {
    HStack(spacing: 6) {
        ModelBadge(kind: .noJSON)
        ModelBadge(kind: .tools)
        ModelBadge(kind: .reasoning)
        ModelBadge(kind: .vision)
        ModelBadge(kind: .openWeights)
        ModelBadge(kind: .free)
    }
    .padding()
}
