import SwiftUI

// MARK: - Wiki page view

/// Single-page renderer with an editorial layout.
///
/// Features hairline dividers, single column at ~62 ch on phones.
/// Uses the standard app theme for visual consistency.
///
/// Toolbar actions: regenerate (re-runs `WikiGenerator` with the same
/// topic + scope and atomically swaps the on-disk page) and delete
/// (removes the page from storage).
struct WikiPageView: View {

    let page: WikiPage
    let storage: WikiStorage
    let onDeleted: (UUID) -> Void
    let onRegenerated: (WikiPage) -> Void

    @Environment(AppStateStore.self) private var store
    @State private var peeking: WikiCitation?
    @State private var isRegenerating = false
    @State private var actionError: String?
    @State private var showDeleteConfirm = false

    init(
        page: WikiPage,
        storage: WikiStorage = .shared,
        onDeleted: @escaping (UUID) -> Void = { _ in },
        onRegenerated: @escaping (WikiPage) -> Void = { _ in }
    ) {
        self.page = page
        self.storage = storage
        self.onDeleted = onDeleted
        self.onRegenerated = onRegenerated
    }

    var body: some View {
        ScrollView {
            VStack(alignment: .leading, spacing: 24) {
                header
                summary
                if let actionError {
                    Label(actionError, systemImage: "exclamationmark.triangle")
                        .font(.caption)
                        .foregroundStyle(AppTheme.Tint.error)
                }
                ForEach(page.sections.sorted(by: { $0.ordinal < $1.ordinal })) { section in
                    sectionView(section)
                }
                citationsList
                footer
            }
            .padding(.horizontal, 20)
            .padding(.vertical, 24)
            .frame(maxWidth: .infinity, alignment: .leading)
        }
        .scrollIndicators(.hidden)
        .background(Color(.systemGroupedBackground))
        .navigationTitle(page.title)
        .navigationBarTitleDisplayMode(.inline)
        .toolbar { toolbar }
        .sheet(item: $peeking) { citation in
            CitationPeekSheet(
                citation: citation,
                resolveEpisode: { [store] id in store.episode(id: id) }
            )
            .presentationDetents([.fraction(0.42), .medium])
            .presentationDragIndicator(.visible)
            .presentationBackground(.regularMaterial)
        }
        // `.alert` rather than `.confirmationDialog` — iOS 26 promotes
        // dialogs anchored close to a tappable element (the toolbar Menu's
        // Delete button below) into popovers and elides the Cancel
        // button. See same fix in ShowDetailView, StorageSettingsView,
        // EpisodeDetailActionsMenu, PlayerQueueSheet, DataExportView.
        .alert(
            "Delete this wiki page?",
            isPresented: $showDeleteConfirm
        ) {
            Button("Cancel", role: .cancel) {}
            Button("Delete", role: .destructive) { performDelete() }
        } message: {
            Text("\(page.title) will be removed from your library.")
        }
    }

    // MARK: - Toolbar

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Menu {
                Button {
                    Task { await regenerate() }
                } label: {
                    Label("Regenerate", systemImage: "arrow.clockwise")
                }
                .disabled(isRegenerating)

                Button(role: .destructive) {
                    showDeleteConfirm = true
                } label: {
                    Label("Delete", systemImage: "trash")
                }
            } label: {
                if isRegenerating {
                    ProgressView()
                } else {
                    Image(systemName: "ellipsis.circle")
                }
            }
            .accessibilityLabel("Page actions")
        }
    }

    // MARK: - Sections

    private var header: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(page.title)
                .font(AppTheme.Typography.largeTitle)
                .tracking(-0.4)
                .foregroundStyle(.primary)
            Text(metadataLine)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.5)
        }
        .padding(.top, 8)
        .accessibilityElement(children: .combine)
        .accessibilityLabel("\(page.title), \(page.kind.displayName)")
    }

    private var summary: some View {
        Text(page.summary)
            .font(AppTheme.Typography.body)
            .italic()
            .foregroundStyle(.primary)
            .lineSpacing(4)
    }

    private func sectionView(_ section: WikiSection) -> some View {
        VStack(alignment: .leading, spacing: 14) {
            Divider()
                .overlay(Color.primary.opacity(0.18))
            Text(section.heading)
                .font(.caption)
                .foregroundStyle(.secondary)
                .textCase(.uppercase)
                .tracking(0.6)
            if let note = section.editorialNote {
                Text(note)
                    .font(.caption2)
                    .foregroundStyle(.tertiary)
                    .italic()
            }
            ForEach(section.claims) { claim in
                claimView(claim)
            }
        }
    }

    private func claimView(_ claim: WikiClaim) -> some View {
        VStack(alignment: .leading, spacing: 8) {
            Text(claim.text)
                .font(AppTheme.Typography.body)
                .lineSpacing(4)
                .foregroundStyle(.primary)
            if !claim.citations.isEmpty {
                citationChips(for: claim)
            }
            if claim.isContestedByUser {
                Label("You flagged this", systemImage: "exclamationmark.bubble")
                    .font(.caption2)
                    .foregroundStyle(AppTheme.Tint.editorialAmber)
            }
        }
        .modifier(EvidenceGradedRule(grade: claim.evidenceGrade))
        .accessibilityElement(children: .combine)
        .accessibilityLabel(claim.text)
        .accessibilityValue(claim.confidence.accessibilityValue)
    }

    private func citationChips(for claim: WikiClaim) -> some View {
        FlexibleChipRow(items: claim.citations) { citation in
            CitationChip(
                citation: citation,
                resolveEpisode: { [store] id in store.episode(id: id) }
            )
        }
    }

    @ViewBuilder
    private var citationsList: some View {
        if !page.citations.isEmpty {
            VStack(alignment: .leading, spacing: 14) {
                Divider().overlay(Color.primary.opacity(0.18))
                Text("Citations")
                    .font(.caption)
                    .foregroundStyle(.secondary)
                    .textCase(.uppercase)
                    .tracking(0.6)
                ForEach(page.citations) { citation in
                    Button {
                        peeking = citation
                    } label: {
                        HStack(alignment: .top, spacing: 10) {
                            Text(citation.formattedTimestamp)
                                .font(.system(.caption, design: .monospaced))
                                .foregroundStyle(AppTheme.Tint.editorialAmber)
                                .frame(width: 64, alignment: .leading)
                            VStack(alignment: .leading, spacing: 2) {
                                if let speaker = citation.speaker {
                                    Text(speaker)
                                        .font(.caption.weight(.medium))
                                        .foregroundStyle(.primary)
                                }
                                Text("\u{201C}\(citation.quoteSnippet)\u{201D}")
                                    .font(AppTheme.Typography.caption)
                                    .italic()
                                    .foregroundStyle(.secondary)
                                    .multilineTextAlignment(.leading)
                            }
                        }
                        .padding(.vertical, 6)
                        .frame(maxWidth: .infinity, alignment: .leading)
                    }
                    .buttonStyle(.plain)
                }
            }
        }
    }

    private var footer: some View {
        VStack(alignment: .leading, spacing: 4) {
            Divider().overlay(Color.primary.opacity(0.10))
            HStack {
                Text("rev \(page.compileRevision) \u{00B7} \(page.model)")
                Spacer()
                Text(page.generatedAt, format: .relative(presentation: .named))
            }
            .font(.caption2)
            .foregroundStyle(.tertiary)
            .padding(.top, 4)
        }
    }

    // MARK: - Actions

    @MainActor
    private func regenerate() async {
        guard !isRegenerating else { return }
        isRegenerating = true
        actionError = nil
        defer { isRegenerating = false }
        do {
            let reference = LLMModelReference(storedID: page.model)
            guard LLMProviderCredentialResolver.hasAPIKey(for: reference.provider) else {
                actionError = LLMProviderCredentialResolver.missingCredentialMessage(for: reference.provider)
                return
            }
            let generator = WikiGenerator(
                rag: RAGService.shared.wikiRAG,
                client: .live(model: page.model),
                storage: storage,
                model: page.model
            )
            let result = try await generator.audit(prior: page)
            try generator.persist(result.page)
            onRegenerated(result.page)
        } catch {
            actionError = (error as? WikiClientError)?.errorDescription
                ?? error.localizedDescription
        }
    }

    private func performDelete() {
        do {
            try storage.delete(slug: page.slug, scope: page.scope)
            onDeleted(page.id)
        } catch {
            actionError = "Could not delete page: \(error.localizedDescription)"
        }
    }

    // MARK: - Helpers

    private var metadataLine: String {
        let count = page.allClaims.flatMap(\.citations).count
        return "\(page.kind.displayName) \u{00B7} \(count) citations \u{00B7} confidence \(Int(page.confidence * 100))%"
    }
}

// MARK: - Flexible chip row

/// Wraps citation chips when they exceed the available width. Keeps the
/// editorial layout clean even on narrow phones.
private struct FlexibleChipRow<Item: Identifiable, Content: View>: View {
    let items: [Item]
    @ViewBuilder let content: (Item) -> Content

    var body: some View {
        LazyVGrid(
            columns: [GridItem(.adaptive(minimum: 84), spacing: 6, alignment: .leading)],
            alignment: .leading,
            spacing: 6
        ) {
            ForEach(items) { item in
                content(item)
            }
        }
    }
}
