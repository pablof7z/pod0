import SwiftUI

struct AIModelsSettingsView: View {
    @Environment(AppStateStore.self) private var store
    @State private var agentSelectorPresented = false
    @State private var thinkingSelectorPresented = false
    @State private var memorySelectorPresented = false
    @State private var wikiSelectorPresented = false
    @State private var categorizationSelectorPresented = false
    @State private var chapterSelectorPresented = false
    @State private var embeddingsSelectorPresented = false
    @State private var catalog = OpenRouterModelSelectorViewModel()

    var body: some View {
        ZStack {
            Color(.systemGroupedBackground)
                .ignoresSafeArea()

            List {
                modelsSection
                speechSection
                retrievalSection
            }
            .listStyle(.insetGrouped)
            .scrollContentBackground(.hidden)
        }
        .navigationTitle("Models")
        .navigationBarTitleDisplayMode(.inline)
        .task {
            await catalog.loadIfNeeded()
            backfillModelNames()
        }
        .sheet(isPresented: $agentSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: agentModelBinding, selectedModelName: agentModelNameBinding, role: "Agent (Initial)")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $thinkingSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: thinkingModelBinding, selectedModelName: thinkingModelNameBinding, role: "Agent (Thinking)")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $memorySelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: memoryModelBinding, selectedModelName: memoryModelNameBinding, role: "Memory Compilation")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $wikiSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: wikiModelBinding, selectedModelName: wikiModelNameBinding, role: "Wiki")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $categorizationSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: categorizationModelBinding, selectedModelName: categorizationModelNameBinding, role: "Categorization")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $chapterSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: chapterModelBinding, selectedModelName: chapterModelNameBinding, role: "Chapter Compilation")
            }
            .presentationDragIndicator(.visible)
        }
        .sheet(isPresented: $embeddingsSelectorPresented) {
            NavigationStack {
                OpenRouterModelSelectorView(selectedModelID: embeddingsModelBinding, selectedModelName: embeddingsModelNameBinding, role: "Embeddings")
            }
            .presentationDragIndicator(.visible)
        }
    }

    // MARK: - Sections

    private var modelsSection: some View {
        Section {
            modelRow(
                icon: "brain.head.profile",
                tint: .orange,
                role: "Agent (Initial)",
                modelID: store.state.settings.agentInitialModel,
                modelName: store.state.settings.agentInitialModelName
            ) {
                agentSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.agentInitialModel))

            modelRow(
                icon: "sparkles",
                tint: .pink,
                role: "Agent (Thinking)",
                modelID: store.state.settings.agentThinkingModel,
                modelName: store.state.settings.agentThinkingModelName
            ) {
                thinkingSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.agentThinkingModel))

            modelRow(
                icon: "memories",
                tint: .purple,
                role: "Memory Compilation",
                modelID: store.state.settings.memoryCompilationModel,
                modelName: store.state.settings.memoryCompilationModelName
            ) {
                memorySelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.memoryCompilationModel))

            modelRow(
                icon: "book.closed.fill",
                tint: .indigo,
                role: "Wiki",
                modelID: store.state.settings.wikiModel,
                modelName: store.state.settings.wikiModelName
            ) {
                wikiSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.wikiModel))

            modelRow(
                icon: "square.grid.2x2.fill",
                tint: .teal,
                role: "Categorization",
                modelID: store.state.settings.categorizationModel,
                modelName: store.state.settings.categorizationModelName
            ) {
                categorizationSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.categorizationModel))

            modelRow(
                icon: "list.bullet.indent",
                tint: .green,
                role: "Chapter Compilation",
                modelID: store.state.settings.chapterCompilationModel,
                modelName: store.state.settings.chapterCompilationModelName
            ) {
                chapterSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.chapterCompilationModel))

            modelRow(
                icon: "rectangle.stack.fill.badge.person.crop",
                tint: .blue,
                role: "Embeddings",
                modelID: store.state.settings.embeddingsModel,
                modelName: store.state.settings.embeddingsModelName
            ) {
                embeddingsSelectorPresented = true
            }
            ModelPreviewCard(model: catalogModel(for: store.state.settings.embeddingsModel))
        } header: {
            Text("Language Roles")
        } footer: {
            Text("Each role can use a different connected provider and model. The agent starts on the Initial model and can self-upgrade to the Thinking model when a task needs more reasoning. Connect provider keys in Providers first.")
        }
    }

    private var speechSection: some View {
        Section("Speech & Media") {
            NavigationLink {
                SpeechModelsSettingsView()
            } label: {
                SettingsRow(
                    icon: "waveform.and.mic",
                    tint: AppTheme.Brand.elevenLabsTint,
                    title: "Speech",
                    subtitle: speechSummary
                )
            }
            NavigationLink {
                ImageGenerationSettingsView()
            } label: {
                SettingsRow(
                    icon: "photo.badge.sparkle",
                    tint: .pink,
                    title: "Image Generation",
                    subtitle: imageGenSummary
                )
            }
        }
    }

    private var retrievalSection: some View {
        Section {
            Toggle(isOn: rerankerBinding) {
                Label("Reranker", systemImage: "list.number")
            }
        } header: {
            Text("Retrieval")
        } footer: {
            Text("Embeddings power on-device search and the agent's evidence retrieval. The reranker reorders top results with a cross-encoder for higher quality at extra token cost.")
        }
    }

    // MARK: - Row helper

    private func modelRow(icon: String, tint: Color, role: String, modelID: String, modelName: String, onTap: @escaping () -> Void) -> some View {
        let displayName = displayName(modelID: modelID, modelName: modelName)
        return Button(action: onTap) {
            SettingsRow(
                icon: icon,
                tint: tint,
                title: role,
                subtitle: displayName
            )
        }
        .buttonStyle(.pressable)
        .accessibilityLabel("\(role), \(displayName)")
        .accessibilityHint("Opens model selector")
    }

    // MARK: - Bindings

    private var agentModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.agentInitialModel },
            set: { v in var s = store.state.settings; s.agentInitialModel = v; store.updateSettings(s) }
        )
    }

    private var agentModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.agentInitialModelName },
            set: { v in var s = store.state.settings; s.agentInitialModelName = v; store.updateSettings(s) }
        )
    }

    private var thinkingModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.agentThinkingModel },
            set: { v in var s = store.state.settings; s.agentThinkingModel = v; store.updateSettings(s) }
        )
    }

    private var thinkingModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.agentThinkingModelName },
            set: { v in var s = store.state.settings; s.agentThinkingModelName = v; store.updateSettings(s) }
        )
    }

    private var memoryModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.memoryCompilationModel },
            set: { v in var s = store.state.settings; s.memoryCompilationModel = v; store.updateSettings(s) }
        )
    }

    private var memoryModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.memoryCompilationModelName },
            set: { v in var s = store.state.settings; s.memoryCompilationModelName = v; store.updateSettings(s) }
        )
    }

    private var wikiModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.wikiModel },
            set: { v in var s = store.state.settings; s.wikiModel = v; store.updateSettings(s) }
        )
    }

    private var wikiModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.wikiModelName },
            set: { v in var s = store.state.settings; s.wikiModelName = v; store.updateSettings(s) }
        )
    }

    private var categorizationModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.categorizationModel },
            set: { v in var s = store.state.settings; s.categorizationModel = v; store.updateSettings(s) }
        )
    }

    private var categorizationModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.categorizationModelName },
            set: { v in var s = store.state.settings; s.categorizationModelName = v; store.updateSettings(s) }
        )
    }

    private var chapterModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.chapterCompilationModel },
            set: { v in var s = store.state.settings; s.chapterCompilationModel = v; store.updateSettings(s) }
        )
    }

    private var chapterModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.chapterCompilationModelName },
            set: { v in var s = store.state.settings; s.chapterCompilationModelName = v; store.updateSettings(s) }
        )
    }

    private var embeddingsModelBinding: Binding<String> {
        Binding(
            get: { store.state.settings.embeddingsModel },
            set: { v in var s = store.state.settings; s.embeddingsModel = v; store.updateSettings(s) }
        )
    }

    private var embeddingsModelNameBinding: Binding<String> {
        Binding(
            get: { store.state.settings.embeddingsModelName },
            set: { v in var s = store.state.settings; s.embeddingsModelName = v; store.updateSettings(s) }
        )
    }

    private var rerankerBinding: Binding<Bool> {
        Binding(
            get: { store.state.settings.rerankerEnabled },
            set: { v in
                var s = store.state.settings
                s.rerankerEnabled = v
                store.updateSettings(s)
                Haptics.selection()
            }
        )
    }

    // MARK: - Helpers

    /// Returns the stored human-readable name when available, falling back to
    /// the path-stripped model ID slug for backward compatibility.
    private func displayName(modelID: String, modelName: String) -> String {
        Settings.modelDisplayName(modelID: modelID, modelName: modelName)
    }

    /// Backfills stored model names from the catalog for existing installs
    /// that have a model ID but no persisted name.
    private func backfillModelNames() {
        var s = store.state.settings
        var changed = false
        if s.agentInitialModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.agentInitialModel }) {
            s.agentInitialModelName = match.name
            changed = true
        }
        if s.agentThinkingModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.agentThinkingModel }) {
            s.agentThinkingModelName = match.name
            changed = true
        }
        if s.memoryCompilationModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.memoryCompilationModel }) {
            s.memoryCompilationModelName = match.name
            changed = true
        }
        if s.wikiModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.wikiModel }) {
            s.wikiModelName = match.name
            changed = true
        }
        if s.categorizationModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.categorizationModel }) {
            s.categorizationModelName = match.name
            changed = true
        }
        if s.chapterCompilationModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.chapterCompilationModel }) {
            s.chapterCompilationModelName = match.name
            changed = true
        }
        if s.embeddingsModelName.isEmpty, let match = catalog.models.first(where: { $0.id == s.embeddingsModel }) {
            s.embeddingsModelName = match.name
            changed = true
        }
        if changed { store.updateSettings(s) }
    }

    private var speechSummary: String {
        let s = store.state.settings
        let stt = s.elevenLabsSTTModel.isBlank ? "STT not set" : s.elevenLabsSTTModel
        let tts = s.elevenLabsTTSModel.isBlank ? "TTS not set" : s.elevenLabsTTSModel
        return "\(stt) · \(tts)"
    }

    private var imageGenSummary: String {
        let s = store.state.settings
        return Settings.modelDisplayName(modelID: s.imageGenerationModel, modelName: s.imageGenerationModelName)
    }

    private func catalogModel(for modelID: String) -> OpenRouterModelOption? {
        let id = modelID.trimmed
        guard !id.isEmpty else { return nil }
        return catalog.models.first { $0.id == id }
    }

    // MARK: - ModelPreviewCard

    /// A compact info strip shown beneath each model-role row.
    /// Shows context-window size, pricing tier, and key capability badges.
    /// Renders nothing when the model hasn't been loaded from the catalog yet.
    private struct ModelPreviewCard: View {

        private enum Layout {
            static let chipCornerRadius: CGFloat = 6
            static let chipPaddingH: CGFloat = 6
            static let chipPaddingV: CGFloat = 3
            static let rowSpacing: CGFloat = 6
            static let chipIconSize: CGFloat = 9
            static let chipLabelSize: CGFloat = 11
            static let chipInnerSpacing: CGFloat = 3
        }

        let model: OpenRouterModelOption?

        var body: some View {
            if let model {
                HStack(spacing: Layout.rowSpacing) {
                    if let ctx = model.contextLength {
                        chip(contextLabel(ctx), icon: "text.alignleft", color: .blue)
                    }
                    chip(model.compactPricing, icon: "dollarsign", color: pricingColor(model))
                    if model.supportsTools {
                        chip("Tools", icon: "wrench.and.screwdriver", color: .teal)
                    }
                    if model.supportsReasoning {
                        chip("Reasoning", icon: "brain", color: .purple)
                    }
                    Spacer(minLength: 0)
                }
                .padding(.leading, SettingsRow.contentLeadingInset)
                .padding(.bottom, AppTheme.Spacing.xs)
                .accessibilityElement(children: .combine)
                .accessibilityLabel(accessibilityDescription(model))
            }
        }

        private func chip(_ label: String, icon: String, color: Color) -> some View {
            HStack(spacing: Layout.chipInnerSpacing) {
                Image(systemName: icon)
                    .font(.system(size: Layout.chipIconSize, weight: .semibold))
                Text(label)
                    .font(.system(size: Layout.chipLabelSize, weight: .medium))
            }
            .foregroundStyle(color)
            .padding(.horizontal, Layout.chipPaddingH)
            .padding(.vertical, Layout.chipPaddingV)
            .background(color.opacity(0.12), in: RoundedRectangle(cornerRadius: Layout.chipCornerRadius, style: .continuous))
        }

        private func contextLabel(_ tokens: Int) -> String {
            if tokens >= 1_000_000 { return "\(tokens / 1_000_000)M ctx" }
            if tokens >= 1_000     { return "\(tokens / 1_000)K ctx" }
            return "\(tokens) ctx"
        }

        private func pricingColor(_ model: OpenRouterModelOption) -> Color {
            if model.isFree { return .green }
            if let cost = model.promptCostPerMillion, cost < 1 { return .secondary }
            return .orange
        }

        private func accessibilityDescription(_ model: OpenRouterModelOption) -> String {
            var parts: [String] = []
            if let ctx = model.contextLength { parts.append(contextLabel(ctx)) }
            parts.append(model.compactPricing)
            if model.supportsTools     { parts.append("supports tools") }
            if model.supportsReasoning { parts.append("supports reasoning") }
            return parts.joined(separator: ", ")
        }
    }
}

struct LLMSettingsView: View {
    var body: some View {
        AIModelsSettingsView()
    }
}
