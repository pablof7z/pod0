import SwiftUI

// MARK: - AddShowSheet

/// Modal "+ Add Show" sheet for the Library tab. Three segments:
///
///   - **Search**   — Apple Podcasts directory search → one-tap subscribe,
///                    dispatched through the NMP `podcast.search_itunes` action.
///   - **From URL** — paste / type a feed URL → `podcast.subscribe` dispatch.
///   - **OPML**     — pick an OPML file → `podcast.import_opml` dispatch;
///                    also surfaces a `ShareLink` over the current library
///                    rendered as an OPML 2.0 document.
struct AddShowSheet: View {

    enum Mode: String, CaseIterable, Identifiable {
        case search = "Search"
        case url = "From URL"
        case opml = "OPML"
        var id: String { rawValue }
    }

    let onDismiss: () -> Void

    @State private var mode: Mode = .search

    var body: some View {
        NavigationStack {
            VStack(spacing: AppTheme.Spacing.lg) {
                LiquidGlassSegmentedPicker(
                    "Add show source",
                    selection: $mode,
                    segments: Mode.allCases.map { ($0, $0.rawValue) }
                )
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.top, AppTheme.Spacing.md)

                Group {
                    switch mode {
                    case .search:
                        DiscoverSearchForm(onAdded: handleAdded)
                    case .url:
                        AddByURLForm(onAdded: handleAddedFromURL)
                    case .opml:
                        OPMLTab(onImported: handleAdded)
                    }
                }

                Spacer(minLength: 0)
            }
            .navigationTitle("Add Show")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar { toolbar }
        }
    }

    @ToolbarContentBuilder
    private var toolbar: some ToolbarContent {
        ToolbarItem(placement: .topBarTrailing) {
            Button {
                Haptics.light()
                onDismiss()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .font(.title3)
                    .foregroundStyle(.secondary)
            }
            .accessibilityLabel("Close")
        }
    }

    private func handleAdded() {
        // Search lets users add multiple shows per session — don't auto-dismiss.
        Haptics.success()
    }

    private func handleAddedFromURL() {
        // From-URL is a single-shot flow — close on success.
        Haptics.success()
        onDismiss()
    }
}

// MARK: - AddByURLForm

/// "From URL" segment body. Dispatches `podcast.subscribe` with the typed URL
/// and dismisses via `onAdded` on success.
struct AddByURLForm: View {

    let onAdded: () -> Void

    @Environment(KernelModel.self) private var model

    @State private var feedURL: String = ""
    @State private var isWorking: Bool = false
    @State private var errorMessage: String?

    var body: some View {
        VStack(alignment: .leading, spacing: AppTheme.Spacing.lg) {
            VStack(alignment: .leading, spacing: AppTheme.Spacing.xs) {
                Text("Feed URL")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)

                TextField("https://example.com/feed.rss", text: $feedURL)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .submitLabel(.go)
                    .onSubmit { Task { await submit() } }
                    .padding(AppTheme.Spacing.md)
                    .background(
                        RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                            .fill(Color(.secondarySystemBackground))
                    )

                Button { paste() } label: {
                    Label("Paste from clipboard", systemImage: "doc.on.clipboard")
                        .font(AppTheme.Typography.caption)
                }
                .buttonStyle(.borderless)
            }

            if let errorMessage {
                Label(errorMessage, systemImage: "exclamationmark.triangle.fill")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.error)
            }

            Button {
                Task { await submit() }
            } label: {
                HStack {
                    if isWorking { ProgressView().controlSize(.small) }
                    Text(isWorking ? "Fetching feed…" : "Subscribe")
                        .frame(maxWidth: .infinity)
                }
                .padding(.vertical, AppTheme.Spacing.sm)
            }
            .buttonStyle(.glassProminent)
            .disabled(isWorking || feedURL.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
        }
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private func paste() {
        guard let text = UIPasteboard.general.string else { return }
        feedURL = text.trimmingCharacters(in: .whitespacesAndNewlines)
        Haptics.selection()
    }

    private func submit() async {
        let trimmed = feedURL.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty, !isWorking else { return }
        guard URL(string: trimmed) != nil else {
            errorMessage = "Invalid URL"
            Haptics.warning()
            return
        }
        isWorking = true
        errorMessage = nil

        let result = model.dispatch(
            namespace: "podcast",
            body: ["op": "subscribe", "feed_url": trimmed])

        isWorking = false
        switch result {
        case .accepted:
            onAdded()
        case .failure(let message):
            errorMessage = message
            Haptics.warning()
        }
    }
}
