import Observation
import SwiftUI

/// Full model browser presented as a sheet.
/// Wrap in a `NavigationStack` at the call site.
struct OpenRouterModelSelectorView: View {

    private enum Layout {
        static let maxProviderCount: Int = 24
        static let pinnedProviderIDs: Set<String> = ["ollama-cloud"]
        static let currentFallbackSpacing: CGFloat = 6
        static let rowVerticalPadding: CGFloat = 4
        static let loadingSpacing: CGFloat = 12
        static let errorSpacing: CGFloat = 10
        static let chipHPadding: CGFloat = 10
        static let chipVPadding: CGFloat = 6
    }

    @Binding var selectedModelID: String
    /// Persisted human-readable name for the selected model, updated on every selection.
    @Binding var selectedModelName: String
    /// Human-readable role label forwarded to the detail view (e.g. "Agent", "Memory Compilation").
    var role: String = "Model"
    /// Pre-selected capability filter chip shown when the sheet opens.
    var initialCapabilityFilter: ModelCapabilityFilter = .compatible
    @Environment(\.dismiss) private var dismiss

    @State private var viewModel = OpenRouterModelSelectorViewModel()
    @State private var searchText = ""
    @State private var capabilityFilter: ModelCapabilityFilter
    @State private var sort: ModelSort = .recommended
    @State private var providerFilter: String?
    @State private var manualModelID = ""

    init(
        selectedModelID: Binding<String>,
        selectedModelName: Binding<String>,
        role: String = "Model",
        initialCapabilityFilter: ModelCapabilityFilter = .compatible
    ) {
        self._selectedModelID = selectedModelID
        self._selectedModelName = selectedModelName
        self.role = role
        self.initialCapabilityFilter = initialCapabilityFilter
        self._capabilityFilter = State(initialValue: initialCapabilityFilter)
    }

    var body: some View {
        List {
            currentSection
            controlsSection
            loadingSection
            modelsSection
            customSection
        }
        .listStyle(.insetGrouped)
        .navigationTitle("\(role) Model")
        .navigationBarTitleDisplayMode(.inline)
        .searchable(text: $searchText, prompt: "Search models, providers, ids")
        .refreshable { await viewModel.reload() }
        .task {
            if manualModelID.isEmpty { manualModelID = selectedModelID }
            await viewModel.loadIfNeeded()
        }
        .navigationDestination(for: OpenRouterModelOption.self) { model in
            OpenRouterModelDetailView(model: model, selectedModelID: $selectedModelID, selectedModelName: $selectedModelName, role: role)
        }
        .toolbar {
            ToolbarItem(placement: .cancellationAction) {
                Button("Done") { dismiss() }
            }
            ToolbarItemGroup(placement: .primaryAction) {
                providerMenu
                Button {
                    Task { await viewModel.reload() }
                } label: {
                    Image(systemName: "arrow.clockwise")
                }
                .disabled(viewModel.isLoading)
                .accessibilityLabel("Refresh models")
            }
        }
    }

    // MARK: - Sections

    private var currentSection: some View {
        Section("Current") {
            if let current = viewModel.models.first(where: { $0.id == selectedModelID }) {
                NavigationLink(value: current) {
                    OpenRouterModelRow(model: current, isSelected: true)
                }
            } else {
                VStack(alignment: .leading, spacing: Layout.currentFallbackSpacing) {
                    Text(selectedModelID)
                        .font(AppTheme.Typography.monoSubheadline)
                    Text("Custom model ID")
                        .font(AppTheme.Typography.caption)
                        .foregroundStyle(.secondary)
                }
                .padding(.vertical, Layout.rowVerticalPadding)
            }
        }
    }

    private var controlsSection: some View {
        Section {
            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: AppTheme.Spacing.xs) {
                    ForEach(ModelCapabilityFilter.allCases) { filter in
                        let isSelected = capabilityFilter == filter
                        Button {
                            capabilityFilter = filter
                            Haptics.selection()
                        } label: {
                            Label(filter.title, systemImage: filter.systemImage)
                                .font(AppTheme.Typography.caption.weight(isSelected ? .semibold : .regular))
                                .foregroundStyle(isSelected ? Color.white : Color.primary)
                                .padding(.horizontal, Layout.chipHPadding)
                                .padding(.vertical, Layout.chipVPadding)
                                .background(isSelected ? Color.accentColor : Color.secondary.opacity(0.12), in: Capsule())
                        }
                        .buttonStyle(.plain)
                        .accessibilityAddTraits(isSelected ? .isSelected : [])
                    }
                }
                .padding(.horizontal, AppTheme.Spacing.md)
                .padding(.vertical, AppTheme.Spacing.xs)
            }
            .listRowInsets(.init())
            .listRowBackground(Color.clear)
            .listRowSeparator(.hidden)
            .animation(AppTheme.Animation.springFast, value: capabilityFilter)

            Picker("Sort", selection: $sort) {
                ForEach(ModelSort.allCases) { s in
                    Text(s.title).tag(s)
                }
            }

            if let providerFilter,
               let name = viewModel.models.first(where: { $0.providerID == providerFilter })?.providerName {
                Button {
                    self.providerFilter = nil
                } label: {
                    Label("Provider: \(name)", systemImage: "xmark.circle")
                }
            }
        }
    }

    @ViewBuilder
    private var loadingSection: some View {
        if viewModel.isLoading && viewModel.models.isEmpty {
            Section {
                HStack(spacing: Layout.loadingSpacing) {
                    ProgressView()
                    Text("Loading models")
                        .foregroundStyle(.secondary)
                }
            }
        }

        if let error = viewModel.errorMessage {
            Section {
                VStack(alignment: .leading, spacing: Layout.errorSpacing) {
                    Label(error, systemImage: "exclamationmark.triangle")
                        .font(AppTheme.Typography.subheadline)
                        .foregroundStyle(AppTheme.Tint.warning)

                    Button {
                        Task { await viewModel.reload() }
                    } label: {
                        Label("Try again", systemImage: "arrow.clockwise")
                    }
                }
                .padding(.vertical, Layout.rowVerticalPadding)
            }
        }
    }

    private var modelsSection: some View {
        Section("\(visibleModels.count) Models") {
            if visibleModels.isEmpty && !viewModel.isLoading {
                Text("No models match this search")
                    .foregroundStyle(.secondary)
            } else {
                ForEach(visibleModels) { model in
                    NavigationLink(value: model) {
                        OpenRouterModelRow(model: model, isSelected: model.id == selectedModelID, query: searchText)
                    }
                }
            }
        }
    }

    private var customSection: some View {
        Section("Custom model ID") {
            TextField("provider/model or ollama:model", text: $manualModelID)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .font(AppTheme.Typography.monoBody)

            Button {
                let trimmed = manualModelID.trimmed
                guard !trimmed.isEmpty else { return }
                selectedModelID = trimmed
                selectedModelName = ""
                dismiss()
            } label: {
                Label("Use custom ID", systemImage: "checkmark.circle")
            }
            .disabled(manualModelID.isBlank)
        }
    }

    // MARK: - Provider menu

    private var providerMenu: some View {
        Menu {
            Button {
                providerFilter = nil
            } label: {
                Label("All providers", systemImage: providerFilter == nil ? "checkmark" : "building.2")
            }

            ForEach(providerSummaries) { provider in
                Button {
                    providerFilter = provider.id
                } label: {
                    if providerFilter == provider.id {
                        Label("\(provider.name) (\(provider.count))", systemImage: "checkmark")
                    } else {
                        Text("\(provider.name) (\(provider.count))")
                    }
                }
            }
        } label: {
            Image(systemName: providerFilter != nil ? "building.2.fill" : "building.2")
                .overlay(alignment: .topTrailing) {
                    if providerFilter != nil {
                        Circle()
                            .fill(Color.accentColor)
                            .frame(width: 7, height: 7)
                            .offset(x: 3, y: -3)
                    }
                }
        }
        .accessibilityLabel(providerFilter != nil ? "Filter by provider — active" : "Filter by provider")
    }

    // MARK: - Computed

    private var visibleModels: [OpenRouterModelOption] {
        var models = viewModel.models

        if let providerFilter {
            models = models.filter { $0.providerID == providerFilter }
        }
        models = models.filter { capabilityFilter.matches($0) }

        let terms = searchText.lowercased().split(whereSeparator: \.isWhitespace).map(String.init)
        if !terms.isEmpty {
            models = models.filter { model in
                terms.allSatisfy { model.searchText.contains($0) }
            }
        }

        switch sort {
        case .recommended: return models
        case .newest:  return models.sorted { ($0.createdAt ?? .distantPast) > ($1.createdAt ?? .distantPast) }
        case .price:   return models.sorted { $0.priceSortValue < $1.priceSortValue }
        case .context: return models.sorted { ($0.contextLength ?? 0) > ($1.contextLength ?? 0) }
        case .name:    return models.sorted { $0.name.localizedCaseInsensitiveCompare($1.name) == .orderedAscending }
        }
    }

    private var providerSummaries: [ProviderSummary] {
        let grouped = Dictionary(grouping: viewModel.models, by: \.providerID)
        let summaries: [ProviderSummary] = grouped.map { id, models in
            ProviderSummary(id: id, name: models.first?.providerName ?? id, count: models.count)
        }
        let sorted = summaries.sorted { lhs, rhs in
            if lhs.count != rhs.count { return lhs.count > rhs.count }
            return lhs.name.localizedCaseInsensitiveCompare(rhs.name) == .orderedAscending
        }
        let pinned = sorted.filter { Layout.pinnedProviderIDs.contains($0.id) }
        let remaining = sorted.filter { !Layout.pinnedProviderIDs.contains($0.id) }
        return Array((pinned + remaining).prefix(Layout.maxProviderCount))
    }
}

// MARK: - View model

@MainActor
@Observable
final class OpenRouterModelSelectorViewModel {
    private(set) var models: [OpenRouterModelOption] = []
    private(set) var isLoading = false
    var errorMessage: String?

    private let service = OpenRouterModelCatalogService()

    func loadIfNeeded() async {
        guard models.isEmpty else { return }
        await reload()
    }

    func reload() async {
        isLoading = true
        errorMessage = nil
        defer { isLoading = false }
        do {
            models = try await service.fetchModels()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}
