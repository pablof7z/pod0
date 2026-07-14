import Foundation

// MARK: - Capability filter

enum ModelCapabilityFilter: String, CaseIterable, Identifiable {
    case compatible
    case all
    case free
    case tools
    case reasoning
    case vision
    case imageOutput
    case openWeights

    var id: String { rawValue }

    var title: String {
        switch self {
        case .compatible:  return "Compatible"
        case .all:         return "All"
        case .free:        return "Free"
        case .tools:       return "Tools"
        case .reasoning:   return "Reasoning"
        case .vision:      return "Vision"
        case .imageOutput: return "Image generation"
        case .openWeights: return "Open weights"
        }
    }

    var systemImage: String {
        switch self {
        case .compatible:  return "curlybraces"
        case .all:         return "line.3.horizontal.decrease.circle"
        case .free:        return "dollarsign.circle"
        case .tools:       return "wrench.and.screwdriver"
        case .reasoning:   return "brain"
        case .vision:      return "eye"
        case .imageOutput: return "photo.badge.sparkle"
        case .openWeights: return "lock.open"
        }
    }

    func matches(_ model: OpenRouterModelOption) -> Bool {
        switch self {
        case .compatible:  return model.isCompatible
        case .all:         return true
        case .free:        return model.isFree
        case .tools:       return model.supportsTools
        case .reasoning:   return model.supportsReasoning
        case .vision:      return model.inputModalities.contains("image")
        case .imageOutput: return model.outputModalities.contains("image")
        case .openWeights: return model.openWeights
        }
    }
}

// MARK: - Sort

enum ModelSort: String, CaseIterable, Identifiable {
    case recommended
    case newest
    case price
    case context
    case name

    var id: String { rawValue }

    var title: String {
        switch self {
        case .recommended: return "Recommended"
        case .newest:      return "Newest"
        case .price:       return "Lowest price"
        case .context:     return "Largest context"
        case .name:        return "Name"
        }
    }
}

// MARK: - Provider summary

struct ProviderSummary: Identifiable, Hashable {
    var id: String
    var name: String
    var count: Int
}

// MARK: - Helpers

func tokenLimit(_ value: Int?) -> String {
    guard let value else { return "Unknown" }
    if value >= 1_000_000 {
        return String(format: "%.1fM tokens", Double(value) / 1_000_000)
    }
    if value >= 1_000 {
        return "\(value / 1_000)K tokens"
    }
    return "\(value) tokens"
}

extension OpenRouterModelOption {
    var priceSortValue: Double {
        guard let p = promptCostPerMillion, let c = completionCostPerMillion else {
            return .greatestFiniteMagnitude
        }
        return p + c
    }
}
