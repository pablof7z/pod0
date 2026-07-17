import SwiftUI

// MARK: - AddShowSheet

/// Modal "+ Add Show" sheet for the Library tab. Three segments:
///
///   - **Search**   — Apple Podcasts directory search → one-tap subscribe.
///                    The default segment because most users discover by
///                    name, not by URL.
///   - **From URL** — paste / type a feed URL → `SubscriptionService.addSubscription`.
///   - **OPML**     — hands off to `OPMLImportSheet` for the file picker
///                    + per-row enrichment flow.
///
/// Surfaces all `SubscriptionService.AddError` cases inline so the user knows
/// whether they pasted a typo, hit a network blip, or are already subscribed.
struct AddShowSheet: View {

    enum Mode: String, CaseIterable, Identifiable {
        case search = "Search"
        case url = "From URL"
        case opml = "OPML"

        var id: String { rawValue }
    }

    let store: AppStateStore
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
                        DiscoverSearchForm(store: store, onAdded: handleAdded)
                    case .url:
                        AddByURLForm(store: store, onAdded: handleAddedFromURL)
                    case .opml:
                        OPMLImportContent(store: store, onDismiss: onDismiss)
                            .padding(.top, -AppTheme.Spacing.md)
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

    private func handleAdded(_ podcast: Podcast) {
        // Intentionally NOT auto-dismissing here. The Search segment lets
        // users add multiple shows in one sitting (and we want them to
        // *see* the row flip to a green checkmark — auto-dismiss made the
        // tap read as a no-op). The "From URL" segment, by contrast, is
        // single-shot, so it dismisses via its own success path.
        Haptics.success()
    }

    /// Single-shot success path for the From-URL segment. Closes the sheet
    /// because that flow always adds exactly one feed at a time and there
    /// is no list-of-rows to re-render with a checkmark.
    private func handleAddedFromURL(_ podcast: Podcast) {
        Haptics.success()
        onDismiss()
    }
}

// MARK: - AddByURLForm

/// "From URL" segment body. Lives next to `AddShowSheet` because the two are
/// always presented together and share the dismissal closure.
struct AddByURLForm: View {

    let store: AppStateStore
    /// Invoked on a successful subscribe so the parent can close the sheet.
    let onAdded: (Podcast) -> Void

    @State private var feedURL: String = ""
    @State private var isWorking: Bool = false
    @State private var error: SubscriptionService.AddError?

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

                Button {
                    paste()
                } label: {
                    Label("Paste from clipboard", systemImage: "doc.on.clipboard")
                        .font(AppTheme.Typography.caption)
                }
                .buttonStyle(.borderless)
            }

            if let error {
                Label(error.localizedDescription, systemImage: "exclamationmark.triangle.fill")
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
        isWorking = true
        error = nil
        let service = SubscriptionService(store: store)
        do {
            let added = try await service.addSubscription(feedURLString: trimmed)
            isWorking = false
            onAdded(added)
        } catch let addError as SubscriptionService.AddError {
            isWorking = false
            // "Already subscribed" is success-like — the show the user
            // wanted is already in their library. Mirror DiscoverSearchForm's
            // behaviour: light haptic, dismiss the sheet via onAdded with
            // the existing record; no angry red banner.
            if case .alreadySubscribed = addError,
               let url = URL(string: trimmed),
               let existing = store.podcast(feedURL: url) {
                Haptics.light()
                onAdded(existing)
                return
            }
            error = addError
            Haptics.warning()
        } catch {
            isWorking = false
            self.error = .transport(error.localizedDescription)
            Haptics.warning()
        }
    }
}
