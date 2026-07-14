import Foundation
import Observation

// MARK: - Wiki home view model

/// Drives `WikiView` — owns the on-disk page list, the search query, and
/// the load lifecycle. All reads go through `WikiStorage`; there are no
/// fixtures or mock fallbacks. An empty store renders the empty state.
@Observable
@MainActor
final class WikiHomeViewModel {

    // MARK: - State

    /// All pages on disk, sorted by `generatedAt` descending. Search and
    /// grouping derive from this list.
    private(set) var recentPages: [WikiPage] = []

    /// Free-text query the search bar binds into. Filters `recentPages`
    /// by title and summary, case-insensitively.
    var searchQuery: String = ""

    /// `true` while `load()` is in flight. Drives the inline progress
    /// indicator on the wiki home.
    private(set) var isLoading: Bool = false

    /// Last error surfaced by a load attempt. Cleared on the next
    /// successful load. The view shows a small banner when non-`nil`.
    private(set) var loadError: String?

    /// Storage source. Defaults to the process-wide `WikiStorage.shared`;
    /// tests may inject a temp-rooted instance.
    let storage: WikiStorage

    init(storage: WikiStorage = .shared) {
        self.storage = storage
    }

    // MARK: - Loading

    /// Reads every page from disk and stores them sorted newest-first.
    /// Safe to call repeatedly — the view triggers it on `.task` and
    /// after the generate sheet completes.
    func load() async {
        isLoading = true
        defer { isLoading = false }
        do {
            let loaded = try await Task.detached(priority: .userInitiated) { [storage] in
                try storage.allPages()
            }.value
            recentPages = loaded.sorted { $0.generatedAt > $1.generatedAt }
            loadError = nil
        } catch {
            recentPages = []
            loadError = error.localizedDescription
        }
    }

    /// Inserts the supplied page into the in-memory list (deduping on
    /// `(scope, slug)`) and re-sorts. Lets the view show the new page
    /// immediately while a fresh `load()` runs in the background.
    func upsert(_ page: WikiPage) {
        recentPages.removeAll { $0.slug == page.slug && $0.scope == page.scope }
        recentPages.insert(page, at: 0)
        recentPages.sort { $0.generatedAt > $1.generatedAt }
    }

    /// Removes the page with the given id from the in-memory list.
    /// Persistent removal is the caller's responsibility (it should call
    /// `WikiStorage.delete(pageID:)` first).
    func remove(pageID: UUID) {
        recentPages.removeAll { $0.id == pageID }
    }

    // MARK: - Derived views

    /// Pages with `isPinned == true`, in `generatedAt` descending order.
    /// Empty when nothing is pinned. No UI currently sets this flag —
    /// the property is ready for a pin-gesture follow-up.
    var pinnedPages: [WikiPage] {
        recentPages.filter(\.isPinned)
    }

    /// Pages filtered by `searchQuery`. Empty query returns the full
    /// list. Match is case-insensitive against title + summary.
    ///
    /// Uses `localizedCaseInsensitiveContains` (Foundation's
    /// Unicode-aware fold) instead of double-lowercasing both sides
    /// and substring-matching. The lowercase-then-contains shape misses
    /// folded cases like German "Straße" matching "STRASSE" — same
    /// bug that bit `HighlightedText`. The locale-aware path also
    /// avoids two `.lowercased()` allocations per row per render.
    var filteredPages: [WikiPage] {
        let trimmed = searchQuery.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return recentPages }
        return recentPages.filter { page in
            page.title.localizedCaseInsensitiveContains(trimmed)
                || page.summary.localizedCaseInsensitiveContains(trimmed)
        }
    }

    /// Pages grouped by recency bucket, ordered Today → Older. Empty
    /// buckets are omitted so the list never renders an empty header.
    var groupedPages: [(bucket: RecencyBucket, pages: [WikiPage])] {
        let calendar = Calendar.current
        let now = Date()
        var buckets: [RecencyBucket: [WikiPage]] = [:]
        for page in filteredPages {
            buckets[RecencyBucket.bucket(for: page.generatedAt, now: now, calendar: calendar), default: []].append(page)
        }
        return RecencyBucket.allCases.compactMap { bucket in
            guard let pages = buckets[bucket], !pages.isEmpty else { return nil }
            return (bucket, pages)
        }
    }
}

// MARK: - Recency bucket

/// The four time bands the wiki home groups pages into. Order is meant
/// to read top-down newest-first.
enum RecencyBucket: String, CaseIterable, Hashable, Sendable {
    case today = "Today"
    case yesterday = "Yesterday"
    case thisWeek = "This Week"
    case older = "Older"

    var title: String { rawValue }

    /// Classifies `date` against `now`. `today` covers the current
    /// calendar day, `yesterday` the prior calendar day, `thisWeek` the
    /// remainder of the same calendar week (per `calendar`), and
    /// `older` everything else.
    static func bucket(
        for date: Date,
        now: Date = Date(),
        calendar: Calendar = .current
    ) -> RecencyBucket {
        if calendar.isDateInToday(date) { return .today }
        if calendar.isDateInYesterday(date) { return .yesterday }
        if calendar.isDate(date, equalTo: now, toGranularity: .weekOfYear) {
            return .thisWeek
        }
        return .older
    }
}
