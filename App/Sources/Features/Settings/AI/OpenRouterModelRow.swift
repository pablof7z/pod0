import SwiftUI

/// A single row in the model browser list.
/// Shows provider logo, model name + ID + capability badges, and compact pricing.
struct OpenRouterModelRow: View {
    var model: OpenRouterModelOption
    var isSelected: Bool
    var query: String = ""

    private enum Layout {
        static let outerSpacing: CGFloat = 12
        static let innerSpacing: CGFloat = 6
        static let badgeSpacing: CGFloat = 6
        static let spacerMinLength: CGFloat = 8
        static let pricingColumnWidth: CGFloat = 86
        static let pricingColumnSpacing: CGFloat = 3
        static let rowVerticalPadding: CGFloat = 4
        /// Maximum number of capability badges to show per row.
        static let maxBadgeCount: Int = 4
        /// Accessibility: "image" modality key in OpenRouter API responses.
        static let imageModality = "image"
    }

    var body: some View {
        HStack(alignment: .top, spacing: Layout.outerSpacing) {
            ProviderLogoView(providerID: model.providerID, providerName: model.providerName, iconURL: model.providerIconURL)

            VStack(alignment: .leading, spacing: Layout.innerSpacing) {
                HStack(alignment: .firstTextBaseline, spacing: Layout.innerSpacing) {
                    Group {
                        if query.isEmpty {
                            Text(model.name)
                        } else {
                            HighlightedText(text: model.name, query: query)
                        }
                    }
                    .font(AppTheme.Typography.subheadline.weight(.semibold))
                    .foregroundStyle(.primary)
                    .lineLimit(2)

                    if isSelected {
                        Image(systemName: "checkmark.circle.fill")
                            .foregroundStyle(Color.accentColor)
                            .imageScale(.small)
                    }
                }

                Text(model.id)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                    .truncatedMiddle()

                if !badges.isEmpty {
                    HStack(spacing: Layout.badgeSpacing) {
                        ForEach(badges.prefix(Layout.maxBadgeCount), id: \.self) { badge in
                            ModelBadge(kind: badge)
                        }
                    }
                }
            }

            Spacer(minLength: Layout.spacerMinLength)

            VStack(alignment: .trailing, spacing: Layout.pricingColumnSpacing) {
                Text(model.compactPricing)
                    .font(AppTheme.Typography.caption.weight(.semibold))
                    .multilineTextAlignment(.trailing)
                    .foregroundStyle(.primary)
                Text("per 1M")
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
                Text(tokenLimit(model.contextLength))
                    .font(AppTheme.Typography.caption2)
                    .foregroundStyle(.secondary)
            }
            .frame(width: Layout.pricingColumnWidth, alignment: .trailing)
        }
        .padding(.vertical, Layout.rowVerticalPadding)
    }

    private var badges: [ModelBadgeKind] {
        var result: [ModelBadgeKind] = []
        if !model.isCompatible { result.append(.noJSON) }
        if model.supportsTools { result.append(.tools) }
        if model.supportsReasoning { result.append(.reasoning) }
        if model.inputModalities.contains(Layout.imageModality) { result.append(.vision) }
        if model.openWeights { result.append(.openWeights) }
        if model.isFree { result.append(.free) }
        return result
    }
}

// MARK: - Preview

#Preview {
    List {
        OpenRouterModelRow(
            model: OpenRouterModelOption.preview,
            isSelected: true
        )
    }
    .listStyle(.insetGrouped)
}

private extension OpenRouterModelOption {
    static let preview = OpenRouterModelOption(remote: ProviderModelOptionDTO(
        provider: .openRouter,
        id: "openai/gpt-4o",
        name: "GPT-4o",
        providerID: "openai",
        providerName: "OpenAI",
        providerIconURL: nil,
        modelDescription: nil,
        promptCostPerMillion: 2.5,
        completionCostPerMillion: 10,
        cacheReadCostPerMillion: nil,
        cacheWriteCostPerMillion: nil,
        requestCost: nil,
        imageCost: nil,
        webSearchCost: nil,
        contextLength: 128_000,
        outputLimit: 4096,
        inputModalities: ["text", "image"],
        outputModalities: ["text"],
        tokenizer: "cl100k",
        supportsTools: true,
        supportsReasoning: false,
        supportsResponseFormat: true,
        supportsStructuredOutputs: false,
        openWeights: false,
        isModerated: true,
        createdAtEpochSecs: 1_700_000_000,
        knowledgeCutoff: "2024-04",
        releaseDate: nil,
        lastUpdated: nil,
        searchText: "openai/gpt-4o gpt-4o openai"
    ))
}
