import SwiftUI
import os.log

// MARK: - NostrDiscoverForm

/// "Nostr" segment in `AddShowSheet`. Dispatches NIP-F4 (`kind:10154`)
/// discovery queries through the NMP `podcast.discover_nostr` action and
/// reads results from the kernel snapshot
/// (`model.podcastSnapshot?.nostrResults`).
///
/// Mirrors `DiscoverSearchForm` structurally so the two segments feel the
/// same to the user: type → debounce → results list → tap to subscribe.
/// Subscribing reuses the existing `podcast.subscribe` action (the RSS
/// pipeline) — the kernel never grows a separate "subscribe to Nostr
/// podcast" path; instead the `feed_url` tag carries an RSS url that
/// flows through the same code as iTunes search subscribes.
struct NostrDiscoverForm: View {

    nonisolated private static let logger = Logger.app("NostrDiscover")

    let onAdded: () -> Void

    private static let debounceMS: UInt64 = 400
    private static let minAutoSearchChars: Int = 2

    @Environment(KernelModel.self) private var model

    @State private var query: String = ""
    @State private var isSearching: Bool = false
    @State private var results: [NostrShowSummary] = []
    @State private var lastDispatchedQuery: String?
    @State private var searchError: String?
    @State private var subscribingID: String?
    @State private var searchTask: Task<Void, Never>?
    @State private var queryFocused: Bool = false
    @State private var focusTask: Task<Void, Never>?

    var body: some View {
        VStack(alignment: .leading, spacing: 0) {
            searchField

            if let searchError {
                Label(searchError, systemImage: "exclamationmark.triangle.fill")
                    .font(AppTheme.Typography.caption)
                    .foregroundStyle(AppTheme.Tint.error)
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    .padding(.bottom, AppTheme.Spacing.xs)
            }

            if isSearching || !results.isEmpty {
                resultsList
            } else if shouldShowNoResults {
                noResultsState
            } else {
                calmPrompt
            }
        }
        .onAppear { requestSearchFocus(deferred: true) }
        .onDisappear {
            searchTask?.cancel()
            focusTask?.cancel()
            queryFocused = false
        }
        .onChange(of: query) { _, newValue in
            scheduleAutoSearch(for: newValue)
        }
        .onChange(of: model.podcastSnapshot?.nostrResults) { _, newResults in
            guard isSearching, let newResults else { return }
            results = newResults
            isSearching = false
            if let term = lastDispatchedQuery {
                Self.logger.info("nostr results arrived for query=\(term, privacy: .public) count=\(newResults.count)")
            }
        }
        .onChange(of: model.library) { _, _ in
            // Clear subscribing spinner when the new podcast lands in the library.
            if let id = subscribingID,
               let feedUrl = results.first(where: { $0.id == id })?.feedUrl,
               model.library.contains(where: { $0.feedUrl == feedUrl }) {
                subscribingID = nil
            }
        }
    }

    // MARK: - Search field

    private var searchField: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "antenna.radiowaves.left.and.right")
                .foregroundStyle(.secondary)
            DiscoverSearchTextField(
                placeholder: "Search Nostr podcasts",
                text: $query,
                isFocused: $queryFocused,
                onSubmit: runSearch
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            Button { clearQuery() } label: {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(.secondary)
            }
            .buttonStyle(.plain)
            .opacity(query.isEmpty ? 0 : 1)
            .disabled(query.isEmpty)
            .accessibilityHidden(query.isEmpty)
            .accessibilityLabel("Clear search")
        }
        .padding(AppTheme.Spacing.md)
        .background(
            RoundedRectangle(cornerRadius: AppTheme.Corner.md, style: .continuous)
                .fill(Color(.secondarySystemBackground))
        )
        .padding(.horizontal, AppTheme.Spacing.lg)
        .padding(.bottom, AppTheme.Spacing.sm)
        .contentShape(Rectangle())
        .onTapGesture { requestSearchFocus() }
    }

    // MARK: - States

    private var noResultsState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "antenna.radiowaves.left.and.right.slash")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            Text("No shows found on Nostr")
                .font(AppTheme.Typography.headline)
            if let term = lastDispatchedQuery {
                Text("No NIP-F4 podcasts matched \"\(term)\" on the relay.")
                    .font(AppTheme.Typography.subheadline)
                    .foregroundStyle(.secondary)
                    .multilineTextAlignment(.center)
            }
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private var calmPrompt: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 40)
            Image(systemName: "antenna.radiowaves.left.and.right")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.tertiary)
            Text("Discover Nostr podcasts")
                .font(AppTheme.Typography.headline)
            Text("Find NIP-F4 shows published on the Nostr network.")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    private var resultsList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if isSearching {
                    HStack {
                        Spacer()
                        ProgressView().padding(.vertical, AppTheme.Spacing.lg)
                        Spacer()
                    }
                }
                ForEach(results) { result in
                    NostrDiscoverRow(
                        result: result,
                        isSubscribing: subscribingID == result.id,
                        isAlreadySubscribed: isAlreadySubscribed(result),
                        onSubscribe: { subscribe(to: result) }
                    )
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    Divider().padding(.leading, AppTheme.Spacing.lg + 64 + AppTheme.Spacing.md)
                }
            }
            .padding(.bottom, AppTheme.Spacing.xl)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    // MARK: - Logic

    private var normalizedQuery: String {
        query.trimmingCharacters(in: .whitespacesAndNewlines)
    }

    private var shouldShowNoResults: Bool {
        guard searchError == nil,
              let lastDispatchedQuery,
              !lastDispatchedQuery.isEmpty
        else { return false }
        return normalizedQuery == lastDispatchedQuery && results.isEmpty && !isSearching
    }

    private func isAlreadySubscribed(_ result: NostrShowSummary) -> Bool {
        guard let feedUrl = result.feedUrl else { return false }
        return model.library.contains { $0.feedUrl == feedUrl }
    }

    private func requestSearchFocus(deferred: Bool = false) {
        focusTask?.cancel()
        guard deferred else { queryFocused = true; return }
        focusTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled else { return }
            queryFocused = true
        }
    }

    private func clearQuery() {
        query = ""
        results = []
        lastDispatchedQuery = nil
        searchError = nil
        isSearching = false
        requestSearchFocus()
    }

    private func runSearch() {
        let trimmed = normalizedQuery
        guard !trimmed.isEmpty else { return }
        searchTask?.cancel()
        searchTask = Task { dispatchSearch(trimmed) }
    }

    private func scheduleAutoSearch(for raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        searchTask?.cancel()
        if trimmed.isEmpty {
            results = []
            lastDispatchedQuery = nil
            searchError = nil
            isSearching = false
            return
        }
        guard trimmed.count >= Self.minAutoSearchChars else {
            results = []
            lastDispatchedQuery = nil
            return
        }
        searchTask = Task {
            try? await Task.sleep(nanoseconds: Self.debounceMS * 1_000_000)
            if Task.isCancelled { return }
            dispatchSearch(trimmed)
        }
    }

    private func dispatchSearch(_ term: String) {
        lastDispatchedQuery = term
        isSearching = true
        searchError = nil
        Self.logger.info("dispatching discover_nostr query=\(term, privacy: .public)")
        model.dispatch(namespace: "podcast", body: ["op": "discover_nostr", "query": term])
    }

    private func subscribe(to result: NostrShowSummary) {
        guard let feedUrl = result.feedUrl, !feedUrl.isEmpty else {
            searchError = "This show has no RSS feed URL — Nostr-only playback isn't supported yet."
            Self.logger.error("nostr result \(result.title, privacy: .public) has no feedUrl")
            return
        }
        subscribingID = result.id
        Haptics.light()
        model.dispatch(namespace: "podcast", body: ["op": "subscribe", "feed_url": feedUrl])
        onAdded()
    }
}
