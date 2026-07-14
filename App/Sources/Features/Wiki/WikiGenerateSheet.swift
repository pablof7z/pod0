import SwiftUI

// MARK: - Wiki generate sheet

/// "Compile a page about X" sheet (UX-04 §6e). Drives the real compile
/// pipeline: user types a topic, picks a kind + scope, the sheet calls
/// `WikiGenerator` against the selected wiki model provider and writes the
/// result to `WikiStorage`.
///
/// RAG resolves through `RAGService.shared.wikiRAG`, the live sqlite-vec
/// vector store. Pages compile against whatever transcript chunks have been
/// ingested for the user's subscriptions; if the user has no transcripts
/// indexed yet, the verifier drops most claims and the page lands as a
/// low-confidence skeleton.
struct WikiGenerateSheet: View {

    /// Storage destination. Defaults to the shared singleton; tests pass
    /// a temp-rooted instance.
    let storage: WikiStorage

    /// Called once the compile completes successfully and the page has
    /// been persisted. The host view is responsible for refreshing.
    var onCompile: (WikiPage) -> Void

    /// Optional prefill for the `topic` field. Used by callers that hand
    /// off into the sheet with a topic already in hand (e.g. the Home
    /// Related sheet's "Compose a wiki page from these"). Applied once on
    /// first appearance so the user can still edit it.
    var initialTopic: String = ""

    @Environment(\.dismiss) private var dismiss
    @Environment(AppStateStore.self) private var store

    @State private var topic: String = ""
    @State private var selectedKind: WikiPageKind = .topic
    @State private var scopeChoice: ScopeChoice = .global
    @State private var selectedPodcastID: UUID?
    @State private var phase: Phase = .input
    @State private var hasAPIKey = false

    /// Sheet-local UI state. Distinct from `WikiHomeViewModel` because it
    /// is owned per-presentation.
    enum Phase: Equatable {
        case input
        case compiling
        case done(WikiPage)
        case failed(String)
    }

    /// Picker-local scope toggle. We resolve to `WikiScope` only when we
    /// kick off the compile — that lets the podcast picker stay disabled
    /// until the user explicitly chooses "podcast".
    enum ScopeChoice: String, CaseIterable, Hashable {
        case global = "Library"
        case podcast = "Podcast"
    }

    var body: some View {
        NavigationStack {
            Form {
                topicSection
                scopeSection
                statusSection
            }
            .scrollDismissesKeyboard(.interactively)
            .navigationTitle("Compile a page")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .topBarLeading) {
                    Button("Cancel") { dismiss() }
                        .disabled(phase == .compiling)
                }
                ToolbarItem(placement: .topBarTrailing) {
                    generateButton
                }
            }
        }
        .presentationDragIndicator(.visible)
        .onAppear {
            refreshProviderStatus()
            if topic.isEmpty {
                topic = initialTopic
            }
        }
    }

    // MARK: - Sections

    private var topicSection: some View {
        Section {
            TextField("e.g. mitochondrial uncoupling", text: $topic)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .disabled(phase == .compiling)
            LiquidGlassSegmentedPicker(
                "Kind",
                selection: $selectedKind,
                segments: WikiPageKind.allCases
                    .filter { $0 != .index }
                    .map { ($0, $0.displayName) }
            )
            .listRowBackground(Color.clear)
            .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
            .disabled(phase == .compiling)
        } header: {
            Text("Topic")
        } footer: {
            Text("Compiling reads your transcripts, drafts a page, and verifies every claim before it lands.")
                .font(.caption)
        }
    }

    @ViewBuilder
    private var scopeSection: some View {
        Section {
            LiquidGlassSegmentedPicker(
                "Scope",
                selection: $scopeChoice,
                segments: ScopeChoice.allCases.map { ($0, $0.rawValue) }
            )
            .listRowBackground(Color.clear)
            .listRowInsets(AppTheme.Layout.cardRowInsetsSM)
            .disabled(phase == .compiling)

            if scopeChoice == .podcast {
                if store.sortedFollowedPodcasts.isEmpty {
                    Text("Subscribe to at least one podcast to scope a page.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                } else {
                    Picker("Podcast", selection: $selectedPodcastID) {
                        Text("Select…").tag(UUID?.none)
                        ForEach(store.sortedFollowedPodcasts) { sub in
                            Text(sub.title).tag(UUID?.some(sub.id))
                        }
                    }
                    .disabled(phase == .compiling)
                }
            }
        } header: {
            Text("Scope")
        }
    }

    @ViewBuilder
    private var statusSection: some View {
        Section {
            switch phase {
            case .input:
                if !hasAPIKey {
                    // Tap-through deep link rather than plain text — the user
                    // is already in the right mental model ("I want to compile
                    // a page"), so requiring them to back out, find Settings,
                    // find AI, find the provider sub-screen burns the moment.
                    // The link pushes onto the sheet's own NavigationStack so
                    // they configure-and-come-back without losing the topic
                    // they typed.
                    NavigationLink {
                        OpenRouterSettingsView()
                    } label: {
                        Label(
                            "Connect \(wikiProvider.displayName) in Settings to compile pages.",
                            systemImage: "key"
                        )
                        .font(.callout)
                    }
                } else {
                    Text("This will use \(wikiProvider.displayName) credits.")
                        .font(.caption)
                        .foregroundStyle(.secondary)
                }
            case .compiling:
                HStack(spacing: 12) {
                    ProgressView()
                    VStack(alignment: .leading, spacing: 2) {
                        Text("Compiling \u{201C}\(topic)\u{201D}…")
                            .font(.callout)
                        Text("Searching transcripts, drafting, verifying citations.")
                            .font(.caption2)
                            .foregroundStyle(.secondary)
                    }
                }
            case .done(let page):
                doneCard(page: page)
            case .failed(let message):
                VStack(alignment: .leading, spacing: 10) {
                    Label(message, systemImage: "exclamationmark.triangle")
                        .font(.callout)
                        .foregroundStyle(AppTheme.Tint.evidenceLow)
                    Button {
                        Task { await runCompile() }
                    } label: {
                        Label("Retry", systemImage: "arrow.clockwise")
                            .font(.callout.weight(.semibold))
                    }
                    .buttonStyle(.bordered)
                    .disabled(!canGenerate)
                }
            }
        } header: {
            Text("Status")
        }
    }

    private func doneCard(page: WikiPage) -> some View {
        VStack(alignment: .leading, spacing: 6) {
            HStack {
                Image(systemName: "checkmark.seal.fill")
                    .foregroundStyle(AppTheme.Tint.evidenceHigh)
                Text("Page compiled")
                    .font(.headline)
            }
            Text(page.title)
                .font(.system(.title3).weight(.semibold))
            if !page.summary.isEmpty {
                Text(page.summary)
                    .font(.callout)
                    .foregroundStyle(.secondary)
                    .lineLimit(3)
            }
            HStack(spacing: 12) {
                Label("\(page.allClaims.count) claims", systemImage: "text.alignleft")
                Label(
                    "\(page.allClaims.flatMap(\.citations).count) citations",
                    systemImage: "quote.bubble"
                )
            }
            .font(.caption)
            .foregroundStyle(.tertiary)
        }
        .padding(.vertical, 4)
    }

    // MARK: - Toolbar button

    @ViewBuilder
    private var generateButton: some View {
        switch phase {
        case .done:
            Button("Open") {
                if case .done(let page) = phase {
                    onCompile(page)
                    dismiss()
                }
            }
            .bold()
        default:
            Button("Generate") {
                Task { await runCompile() }
            }
            .bold()
            .disabled(!canGenerate)
        }
    }

    private var canGenerate: Bool {
        guard hasAPIKey else { return false }
        if phase == .compiling { return false }
        if topic.trimmingCharacters(in: .whitespaces).isEmpty { return false }
        if scopeChoice == .podcast && selectedPodcastID == nil { return false }
        return true
    }

    // MARK: - Compile

    @MainActor
    private func runCompile() async {
        guard canGenerate else { return }
        let model = store.state.settings.wikiModel
        let trimmed = topic.trimmingCharacters(in: .whitespacesAndNewlines)
        let scope: WikiScope
        switch scopeChoice {
        case .global:
            scope = .global
        case .podcast:
            guard let id = selectedPodcastID else { return }
            scope = .podcast(id)
        }

        phase = .compiling

        do {
            let reference = LLMModelReference(storedID: model)
            guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
                phase = .failed(LLMProviderCredentialResolver.missingCredentialMessage(for: reference.provider))
                return
            }
            let generator = WikiGenerator(
                rag: RAGService.shared.wikiRAG,
                client: .live(model: model),
                storage: storage,
                model: model
            )
            let result = try await compile(
                generator: generator,
                topic: trimmed,
                scope: scope
            )
            try generator.persist(result.page)
            phase = .done(result.page)
        } catch {
            phase = .failed(humanize(error))
        }
    }

    private func compile(
        generator: WikiGenerator,
        topic: String,
        scope: WikiScope
    ) async throws -> WikiVerifyResult {
        switch selectedKind {
        case .topic, .index:
            return try await generator.compileTopic(topic: topic, scope: scope)
        case .person:
            return try await generator.compilePerson(name: topic, scope: scope)
        case .show:
            return try await generator.compileShow(showName: topic, scope: scope)
        }
    }

    private func humanize(_ error: Error) -> String {
        if let wiki = error as? WikiClientError {
            return wiki.errorDescription ?? "Compile failed."
        }
        if let gen = error as? WikiGeneratorError {
            return gen.errorDescription ?? "Compile failed."
        }
        return error.localizedDescription
    }

    private var wikiProvider: LLMProvider {
        LLMModelReference(storedID: store.state.settings.wikiModel).provider
    }

    private func refreshProviderStatus() {
        hasAPIKey = LLMProviderCredentialResolver.hasAPIKey(for: wikiProvider)
    }
}
