import Foundation
import Observation

@Observable
@MainActor
final class PodcastSearchViewModel {
    var query: String = ""
    /// Lags `query` by the debounce interval; drives local + wiki search.
    var debouncedQuery: String = ""
    /// True from the moment a `search` dispatch fires until the store
    /// delivers the first result batch for that query.
    private(set) var isSearchingTranscripts = false
    private(set) var wikiPages: [WikiPage] = []
    private(set) var wikiLoadError: String?

    let wikiStorage: WikiStorage
    /// Dormant fallback for agent + wiki adapters (slice 5 territory).
    /// The Search tab no longer drives `rag.search` — kernel results arrive
    /// reactively via `store.kernel?.podcastSnapshot?.knowledgeSearchResults`.
    private let rag: RAGSearch

    init(rag: RAGSearch? = nil, wikiStorage: WikiStorage = .shared) {
        self.rag = rag ?? RAGService.shared.search
        self.wikiStorage = wikiStorage
    }

    var wikiResults: [PodcastWikiSearchHit] {
        PodcastSearchEngine.wikiResults(query: debouncedQuery, pages: wikiPages)
    }

    func loadWikiPages() async {
        do {
            let storage = wikiStorage
            let pages = try await Task.detached(priority: .utility) {
                try storage.allPages()
            }.value
            wikiPages = pages
            wikiLoadError = nil
        } catch {
            wikiPages = []
            wikiLoadError = error.localizedDescription
        }
    }

    func upsertWikiPage(_ page: WikiPage) {
        wikiPages.removeAll { $0.id == page.id || ($0.slug == page.slug && $0.scope == page.scope) }
        wikiPages.insert(page, at: 0)
    }

    func removeWikiPage(id: UUID) {
        wikiPages.removeAll { $0.id == id }
    }

    // MARK: - Kernel transcript search (Slice 4)

    /// Dispatch a kernel knowledge search or clear, using `store` as the
    /// dispatch bridge. Results arrive reactively on
    /// `store.kernel?.podcastSnapshot?.knowledgeSearchResults` — no polling
    /// (project rule). Keeps the debounce gate from the calling view.
    ///
    /// - Empty / cleared query: dispatches `clear_results`, resets spinner.
    /// - Non-empty query: dispatches `search`, sets `isSearchingTranscripts`.
    ///   The view clears the spinner via `didReceiveKernelResults()` once
    ///   the reactive projection delivers results.
    func searchTranscripts(store: AppStateStore) {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else {
            store.kernel?.dispatch(namespace: "podcast.knowledge",
                                   body: ["op": "clear_results"])
            isSearchingTranscripts = false
            return
        }
        isSearchingTranscripts = true
        store.kernel?.dispatch(namespace: "podcast.knowledge",
                               body: ["op": "search", "query": trimmed])
    }

    /// Called by the view when the kernel projection delivers a new batch.
    /// Clears the `isSearchingTranscripts` indicator.
    func didReceiveKernelResults() {
        isSearchingTranscripts = false
    }
}
