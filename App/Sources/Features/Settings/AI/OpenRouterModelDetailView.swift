import SwiftUI

/// Full-screen detail view for a single OpenRouter model.
/// Pushed via `NavigationLink(value:)` from the model selector.
struct OpenRouterModelDetailView: View {
    var model: OpenRouterModelOption
    @Binding var selectedModelID: String
    /// Persisted human-readable name for the selected model, updated alongside the ID.
    @Binding var selectedModelName: String
    /// Human-readable role label (e.g. "Agent", "Memory Compilation").
    /// Used in the select button so users know which role they are configuring.
    var role: String = "Model"
    @Environment(\.dismiss) private var dismiss

    enum Layout {
        static let contentPadding: CGFloat = 20
        static let sectionSpacing: CGFloat = 18
        static let heroSpacing: CGFloat = 14
        static let heroLogoSize: CGFloat = 52
        static let heroInnerSpacing: CGFloat = 6
        static let groupSpacing: CGFloat = 10
        static let groupInnerSpacing: CGFloat = 8
        static let detailLineMinSpacing: CGFloat = 12
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: Layout.sectionSpacing) {
                heroSection
                capabilityChips
                selectButton

                detailGroup("Pricing") {
                    DetailLine("Prompt", pricingDetail(model.promptCostPerMillion))
                    DetailLine("Completion", pricingDetail(model.completionCostPerMillion))
                    if model.cacheReadCostPerMillion != nil {
                        DetailLine("Cache read", pricingDetail(model.cacheReadCostPerMillion))
                    }
                    if model.cacheWriteCostPerMillion != nil {
                        DetailLine("Cache write", pricingDetail(model.cacheWriteCostPerMillion))
                    }
                    if let webSearchCost = model.webSearchCost {
                        DetailLine("Web search", OpenRouterModelOption.money(webSearchCost))
                    }
                    if let imageCost = model.imageCost {
                        DetailLine("Image", OpenRouterModelOption.money(imageCost))
                    }
                }

                detailGroup("Capabilities") {
                    DetailLine("Compatibility", model.isCompatible ? "JSON response format" : "May not support JSON schema")
                    DetailLine("Input", model.inputModalities.isEmpty ? "Unknown" : model.inputModalities.joined(separator: ", "))
                    DetailLine("Output", model.outputModalities.isEmpty ? "Unknown" : model.outputModalities.joined(separator: ", "))
                    DetailLine("Tools", model.supportsTools ? "Yes" : "No")
                    DetailLine("Reasoning", model.supportsReasoning ? "Yes" : "No")
                    DetailLine("Structured output", model.supportsStructuredOutputs ? "Yes" : "No")
                    DetailLine("Weights", model.openWeights ? "Open" : "Closed")
                }

                detailGroup("Limits") {
                    DetailLine("Context", tokenLimit(model.contextLength))
                    DetailLine("Output", tokenLimit(model.outputLimit))
                    if let tokenizer = model.tokenizer {
                        DetailLine("Tokenizer", tokenizer)
                    }
                    if let isModerated = model.isModerated {
                        DetailLine("Moderated", isModerated ? "Yes" : "No")
                    }
                }

                if model.releaseDate != nil || model.lastUpdated != nil || model.knowledgeCutoff != nil || model.createdAt != nil {
                    detailGroup("Dates") {
                        if let releaseDate = model.releaseDate {
                            DetailLine("Release", releaseDate)
                        }
                        if let lastUpdated = model.lastUpdated {
                            DetailLine("Updated", lastUpdated)
                        }
                        if let knowledgeCutoff = model.knowledgeCutoff {
                            DetailLine("Knowledge cutoff", knowledgeCutoff)
                        }
                        if let createdAt = model.createdAt {
                            DetailLine("Catalog updated", createdAt.formatted(date: .abbreviated, time: .omitted))
                        }
                    }
                }

                if let description = model.modelDescription, !description.isEmpty {
                    detailGroup("Description") {
                        Text(description)
                            .font(AppTheme.Typography.subheadline)
                            .foregroundStyle(.secondary)
                            .textSelection(.enabled)
                    }
                }
            }
            .padding(Layout.contentPadding)
        }
        .navigationTitle("Model")
        .navigationBarTitleDisplayMode(.inline)
    }

    // MARK: - Sub-views

    private var heroSection: some View {
        HStack(alignment: .top, spacing: Layout.heroSpacing) {
            ProviderLogoView(providerID: model.providerID, providerName: model.providerName, size: Layout.heroLogoSize)

            VStack(alignment: .leading, spacing: Layout.heroInnerSpacing) {
                Text(model.name)
                    .font(AppTheme.Typography.title3)
                Text(model.id)
                    .font(AppTheme.Typography.monoCaption)
                    .foregroundStyle(.secondary)
                    .textSelection(.enabled)
                Text(model.providerName)
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
            }
        }
    }

    private var selectButton: some View {
        let alreadySelected = selectedModelID == model.id
        return Button {
            selectedModelID = model.id
            selectedModelName = model.name
            dismiss()
        } label: {
            Label(
                alreadySelected ? "Selected for \(role)" : "Use as \(role)",
                systemImage: "checkmark.circle.fill"
            )
            .frame(maxWidth: .infinity)
        }
        .buttonStyle(.glassProminent)
        .disabled(alreadySelected)
    }

    // MARK: - Helpers

    private func detailGroup<Content: View>(
        _ title: String,
        @ViewBuilder content: () -> Content
    ) -> some View {
        VStack(alignment: .leading, spacing: Layout.groupSpacing) {
            Text(title)
                .font(AppTheme.Typography.headline)
            VStack(alignment: .leading, spacing: Layout.groupInnerSpacing) {
                content()
            }
        }
        .frame(maxWidth: .infinity, alignment: .leading)
    }

    private func pricingDetail(_ value: Double?) -> String {
        guard let value else { return "Variable" }
        return "\(OpenRouterModelOption.perToken(value)) / \(OpenRouterModelOption.money(value)) per 1M"
    }

    // MARK: - Capability chips

    private var capabilityChips: some View {
        HStack(spacing: AppTheme.Spacing.xs) {
            if let ctx = model.contextLength {
                infoChip(contextLabel(ctx), icon: "text.alignleft", color: .blue)
            }
            infoChip(model.compactPricing, icon: "dollarsign", color: pricingColor)
            if model.supportsTools {
                infoChip("Tools", icon: "wrench.and.screwdriver", color: .teal)
            }
            if model.supportsReasoning {
                infoChip("Reasoning", icon: "brain", color: .purple)
            }
            Spacer(minLength: 0)
        }
    }

    private func infoChip(_ label: String, icon: String, color: Color) -> some View {
        HStack(spacing: 3) {
            Image(systemName: icon).font(.system(size: 9, weight: .semibold))
            Text(label).font(.system(size: 11, weight: .medium))
        }
        .foregroundStyle(color)
        .padding(.horizontal, 6)
        .padding(.vertical, 3)
        .background(color.opacity(0.12), in: RoundedRectangle(cornerRadius: 6, style: .continuous))
    }

    private var pricingColor: Color {
        if model.isFree { return .green }
        if let cost = model.promptCostPerMillion, cost < 1 { return .secondary }
        return .orange
    }

    private func contextLabel(_ tokens: Int) -> String {
        if tokens >= 1_000_000 { return "\(tokens / 1_000_000)M ctx" }
        if tokens >= 1_000     { return "\(tokens / 1_000)K ctx" }
        return "\(tokens) ctx"
    }

    // MARK: - DetailLine

    struct DetailLine: View {
        var label: String
        var value: String

        init(_ label: String, _ value: String) {
            self.label = label
            self.value = value
        }

        var body: some View {
            HStack(alignment: .firstTextBaseline) {
                Text(label)
                    .foregroundStyle(.secondary)
                Spacer(minLength: Layout.detailLineMinSpacing)
                Text(value)
                    .multilineTextAlignment(.trailing)
                    .textSelection(.enabled)
            }
            .font(AppTheme.Typography.subheadline)
        }
    }
}
