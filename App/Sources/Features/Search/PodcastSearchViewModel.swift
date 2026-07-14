import Foundation
import Observation

@Observable
@MainActor
final class PodcastSearchViewModel {
    var query: String = ""
    /// Lags `query` by the debounce interval; drives local + wiki search.
    var debouncedQuery: String = ""
    private(set) var transcriptResults: [PodcastTranscriptSearchHit] = []
    private(set) var wikiPages: [WikiPage] = []
    private(set) var isSearchingTranscripts = false
    private(set) var transcriptError: String?
    private(set) var wikiLoadError: String?

    let wikiStorage: WikiStorage
    private let rag: RAGSearch
    private var activeTranscriptQuery: String?

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

    func searchTranscripts() async {
        let trimmed = query.trimmed
        guard !trimmed.isEmpty else {
            activeTranscriptQuery = nil
            transcriptResults = []
            transcriptError = nil
            isSearchingTranscripts = false
            return
        }

        activeTranscriptQuery = trimmed
        isSearchingTranscripts = true
        transcriptError = nil
        defer {
            if activeTranscriptQuery == trimmed {
                isSearchingTranscripts = false
                activeTranscriptQuery = nil
            }
        }

        do {
            // rerank: false — search fires on every debounced keystroke, so
            // the extra ~220 ms the Cohere reranker adds (per RAGSearch.swift
            // latency budget) exceeds the perceptual threshold for live
            // typing. HomeRelatedSheet can afford rerank: true because it
            // runs a single query on sheet open, not per-keystroke.
            let matches = try await rag.search(
                query: trimmed,
                scope: .all,
                options: .init(k: 8, overfetchMultiplier: 3, hybrid: true, rerank: false)
            )
            guard activeTranscriptQuery == trimmed, query.trimmed == trimmed else { return }
            transcriptResults = matches.map { match in
                PodcastTranscriptSearchHit(
                    chunk: match.chunk,
                    score: match.score,
                    snippet: match.chunk.text
                )
            }
        } catch is CancellationError {
            return
        } catch {
            guard activeTranscriptQuery == trimmed, query.trimmed == trimmed else { return }
            transcriptResults = []
            transcriptError = error.localizedDescription
        }
    }
}
