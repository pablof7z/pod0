import SwiftUI
import os.log

// MARK: - DiscoverSearchForm

/// "Search" segment body in `AddShowSheet`. Drives Apple's iTunes Search
/// directory — the standard "discover podcasts" surface every podcast app
/// ships. Tapping a result calls into `SubscriptionService.addSubscription`
/// (same path as the From-URL form, plus the OPML import).
///
/// Tap targets:
///   - The whole row body subscribes (single tap anywhere on the cell).
///   - The trailing ⊕ button does the same thing (and acts as the
///     accessibility action target).
///
/// **State surfaces:**
///   - empty (no query yet)    — calm prompt
///   - searching                — inline spinner
///   - has results              — list of `DiscoverResultRow`
///   - search-level error       — inline label above results
///   - per-row subscribe error  — red ⚠ icon on the row, tap to expand
struct DiscoverSearchForm: View {

    nonisolated private static let logger = Logger.app("AddShowSearch")

    let store: AppStateStore
    let onAdded: (Podcast) -> Void

    /// Wait this long after the user stops typing before firing a search.
    /// Long enough to skip mid-word taps; short enough that results feel live.
    private static let debounceMS: UInt64 = 300

    /// Auto-search only kicks in once the query is meaningful.
    private static let minAutoSearchChars: Int = 2

    @State private var query: String = ""
    @State private var isSearching: Bool = false
    @State private var results: [ITunesSearchClient.Result] = []
    @State private var lastCompletedSearchTerm: String?
    @State private var searchError: String?
    @State private var subscribingID: Int?
    /// Per-row subscribe failure messages, keyed by `collectionId`. Cleared
    /// when the user taps a fresh attempt on the same row.
    @State private var rowErrors: [Int: String] = [:]
    /// Rows whose error caption is currently expanded. Toggling the ⚠
    /// chip adds/removes the row's id here.
    @State private var expandedErrorIDs: Set<Int> = []
    @State private var searchTask: Task<Void, Never>?

    /// Trending podcasts shown when the query is empty — fetched once
    /// per sheet appearance, then cached.
    @State private var trending: [ITunesSearchClient.Result] = []
    @State private var isLoadingTrending: Bool = false
    @State private var trendingFetched: Bool = false

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
                emptyOrTrendingState
            }
        }
        .onAppear {
            requestSearchFocus(deferred: true)
            if !trendingFetched {
                trendingFetched = true
                Task { await loadTrending() }
            }
        }
        .onDisappear {
            searchTask?.cancel()
            focusTask?.cancel()
            queryFocused = false
        }
        .onChange(of: query) { _, newValue in
            scheduleAutoSearch(for: newValue)
        }
    }

    // MARK: - Search field

    private var searchField: some View {
        HStack(spacing: AppTheme.Spacing.sm) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
            DiscoverSearchTextField(
                placeholder: "Search Apple Podcasts",
                text: $query,
                isFocused: $queryFocused,
                onSubmit: runSearch
            )
            .frame(maxWidth: .infinity, alignment: .leading)
            .fixedSize(horizontal: false, vertical: true)
            Button {
                clearQuery()
            } label: {
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

    // MARK: - Empty / results

    /// Pre-search empty area: shows the "Popular Now" rail only while the
    /// query is blank; once the user has searched, an empty result set gets
    /// its own no-results state instead of unrelated trending shows.
    @ViewBuilder
    private var emptyOrTrendingState: some View {
        if normalizedQuery.isEmpty, !trending.isEmpty {
            trendingSection
        } else if normalizedQuery.isEmpty, isLoadingTrending {
            VStack {
                Spacer(minLength: 60)
                ProgressView()
                Spacer()
            }
            .frame(maxWidth: .infinity)
        } else {
            calmPrompt
        }
    }

    private var noResultsState: some View {
        VStack(spacing: AppTheme.Spacing.md) {
            Spacer(minLength: 48)
            Image(systemName: "magnifyingglass")
                .font(.system(size: 40, weight: .light))
                .foregroundStyle(.tertiary)
            Text("No shows found")
                .font(AppTheme.Typography.headline)
            if let lastCompletedSearchTerm {
                Text("Try a different show, host, or topic than \"\(lastCompletedSearchTerm)\".")
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
            Image(systemName: "magnifyingglass")
                .font(.system(size: 44, weight: .light))
                .foregroundStyle(.tertiary)
            Text("Find shows on Apple Podcasts")
                .font(AppTheme.Typography.headline)
            Text("Search by show name, host, or topic.")
                .font(AppTheme.Typography.subheadline)
                .foregroundStyle(.secondary)
                .multilineTextAlignment(.center)
            Spacer(minLength: 0)
        }
        .frame(maxWidth: .infinity)
        .padding(.horizontal, AppTheme.Spacing.lg)
    }

    /// "Popular Now" — top podcasts from Apple's marketing feed. Same row
    /// component (and tap-to-subscribe affordance) as search results.
    private var trendingSection: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                HStack {
                    Text("Popular Now")
                        .font(AppTheme.Typography.headline)
                    Spacer()
                }
                .padding(.horizontal, AppTheme.Spacing.lg)
                .padding(.bottom, AppTheme.Spacing.xs)

                ForEach(trending) { result in
                    DiscoverResultRow(
                        result: result,
                        isSubscribing: subscribingID == result.collectionId,
                        isAlreadySubscribed: isAlreadySubscribed(result),
                        rowError: rowErrors[result.collectionId],
                        isErrorExpanded: expandedErrorIDs.contains(result.collectionId),
                        onSubscribe: { Task { await subscribe(to: result) } },
                        onToggleErrorExpansion: { toggleErrorExpansion(for: result.collectionId) }
                    )
                    .padding(.horizontal, AppTheme.Spacing.lg)
                    Divider().padding(.leading, AppTheme.Spacing.lg + 64 + AppTheme.Spacing.md)
                }
            }
            .padding(.bottom, AppTheme.Spacing.xl)
        }
        .scrollDismissesKeyboard(.interactively)
    }

    private var resultsList: some View {
        ScrollView {
            LazyVStack(spacing: 0) {
                if isSearching {
                    HStack {
                        Spacer()
                        ProgressView()
                            .padding(.vertical, AppTheme.Spacing.lg)
                        Spacer()
                    }
                }
                ForEach(results) { result in
                    DiscoverResultRow(
                        result: result,
                        isSubscribing: subscribingID == result.collectionId,
                        isAlreadySubscribed: isAlreadySubscribed(result),
                        rowError: rowErrors[result.collectionId],
                        isErrorExpanded: expandedErrorIDs.contains(result.collectionId),
                        onSubscribe: { Task { await subscribe(to: result) } },
                        onToggleErrorExpansion: { toggleErrorExpansion(for: result.collectionId) }
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
              let lastCompletedSearchTerm,
              !lastCompletedSearchTerm.isEmpty
        else { return false }
        return normalizedQuery == lastCompletedSearchTerm && results.isEmpty
    }

    private func isAlreadySubscribed(_ result: ITunesSearchClient.Result) -> Bool {
        guard let url = result.feedURL,
              let podcast = store.podcast(feedURL: url) else { return false }
        // Just knowing about the feed (e.g. from a prior external play)
        // shouldn't disable the subscribe row. The check is whether the
        // user actually follows it.
        return store.subscription(podcastID: podcast.id) != nil
    }

    private func requestSearchFocus(deferred: Bool = false) {
        focusTask?.cancel()
        guard deferred else {
            queryFocused = true
            return
        }
        focusTask = Task { @MainActor in
            try? await Task.sleep(nanoseconds: 250_000_000)
            guard !Task.isCancelled else { return }
            queryFocused = true
        }
    }

    private func clearQuery() {
        query = ""
        results = []
        lastCompletedSearchTerm = nil
        searchError = nil
        rowErrors.removeAll()
        expandedErrorIDs.removeAll()
        requestSearchFocus()
    }

    private func toggleErrorExpansion(for id: Int) {
        if expandedErrorIDs.contains(id) {
            expandedErrorIDs.remove(id)
        } else {
            expandedErrorIDs.insert(id)
        }
    }

    /// Submit-handler (return key on the keyboard). Skips the debounce so
    /// users who hit Enter get instant results.
    private func runSearch() {
        let trimmed = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        searchTask?.cancel()
        searchTask = Task { await performSearch(trimmed) }
    }

    /// Debounced as-you-type. Clears results when the field empties, skips
    /// queries shorter than `minAutoSearchChars` (no point hitting the API
    /// for a single letter), and cancels any in-flight task each keystroke.
    private func scheduleAutoSearch(for raw: String) {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        searchTask?.cancel()
        if trimmed.isEmpty {
            results = []
            lastCompletedSearchTerm = nil
            searchError = nil
            isSearching = false
            return
        }
        guard trimmed.count >= Self.minAutoSearchChars else {
            results = []
            lastCompletedSearchTerm = nil
            return
        }
        searchTask = Task {
            try? await Task.sleep(nanoseconds: Self.debounceMS * 1_000_000)
            if Task.isCancelled { return }
            await performSearch(trimmed)
        }
    }

    private func performSearch(_ term: String) async {
        isSearching = true
        searchError = nil
        defer { isSearching = false }
        do {
            let fetched = try await ITunesSearchClient.search(term)
            guard !Task.isCancelled else { return }
            results = fetched
            lastCompletedSearchTerm = term
            // Clear stale per-row errors that don't apply to this result set.
            let fetchedIDs = Set(fetched.map(\.collectionId))
            rowErrors = rowErrors.filter { fetchedIDs.contains($0.key) }
            expandedErrorIDs = expandedErrorIDs.filter { fetchedIDs.contains($0) }
        } catch {
            guard !Task.isCancelled else { return }
            Self.logger.error(
                "iTunes search failed for term \(term, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            self.searchError = error.localizedDescription
            results = []
        }
    }

    /// One-shot trending fetch. Failure is silent — the calm prompt takes
    /// over so the empty state never feels broken when the network is gone.
    private func loadTrending() async {
        isLoadingTrending = true
        defer { isLoadingTrending = false }
        let fetched = (try? await ITunesSearchClient.topPodcasts()) ?? []
        trending = fetched
    }

    private func subscribe(to result: ITunesSearchClient.Result) async {
        guard let feedURL = result.feedURL else {
            // No feedUrl in the iTunes payload — surface as a per-row error
            // so the user understands why this specific show won't subscribe.
            let message = "This show has no public feed URL."
            Self.logger.error(
                "iTunes result \(result.collectionName, privacy: .public) (\(result.collectionId)) has no feedUrl"
            )
            rowErrors[result.collectionId] = message
            return
        }
        // Eager spinner so the tap registers visually even before the
        // network round-trip completes.
        subscribingID = result.collectionId
        rowErrors.removeValue(forKey: result.collectionId)
        expandedErrorIDs.remove(result.collectionId)
        defer { subscribingID = nil }

        let service = SubscriptionService(store: store)
        do {
            let added = try await service.addSubscription(feedURLString: feedURL.absoluteString)
            // Sanity check the write actually landed.
            if store.podcast(feedURL: feedURL) == nil {
                Self.logger.error(
                    "Subscription \(result.collectionName, privacy: .public) reported success but is missing from store"
                )
            }
            Haptics.success()
            // NOTE: parent `AddShowSheet` no longer auto-dismisses, so the
            // user sees this row's checkmark and can keep adding shows.
            onAdded(added)
        } catch let addError as SubscriptionService.AddError {
            // "Already subscribed" is success-like — the row will just flip
            // to the checkmark on next render via `isAlreadySubscribed`.
            if case .alreadySubscribed = addError {
                Haptics.light()
                return
            }
            Self.logger.error(
                "Failed to subscribe to \(result.collectionName, privacy: .public) at \(feedURL.absoluteString, privacy: .public): \(addError.localizedDescription, privacy: .public)"
            )
            rowErrors[result.collectionId] = addError.localizedDescription
            Haptics.warning()
        } catch {
            Self.logger.error(
                "Unexpected error subscribing to \(result.collectionName, privacy: .public): \(error.localizedDescription, privacy: .public)"
            )
            rowErrors[result.collectionId] = error.localizedDescription
            Haptics.warning()
        }
    }
}
