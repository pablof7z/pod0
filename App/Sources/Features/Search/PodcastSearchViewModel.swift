import Foundation
import Observation
import Pod0Core

@Observable
@MainActor
final class PodcastSearchViewModel {
    var query: String = ""
    /// Lags `query` by the debounce interval; drives local + transcript search.
    var debouncedQuery: String = ""
    private(set) var transcriptResults: [PodcastTranscriptSearchHit] = []
    private(set) var isSearchingTranscripts = false
    private(set) var transcriptError: String?

    private weak var recall: (any SharedRecallSearching)?
    private var activeTranscriptQuery: String?

    func attach(recall: (any SharedRecallSearching)?) {
        self.recall = recall
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

        guard let recall else {
            transcriptResults = []
            transcriptError = "Transcript recall is unavailable until Pod0 finishes opening."
            return
        }
        let projection = await recall.recall(query: trimmed, scope: .library, limit: 8)
        guard activeTranscriptQuery == trimmed, query.trimmed == trimmed else { return }
        if projection.stage == .ready {
            transcriptResults = projection.evidence.map(PodcastTranscriptSearchHit.init)
            return
        }
        transcriptResults = []
        transcriptError = switch projection.stage {
        case .noEvidence: nil
        case .transcriptMissing: "Prepare a transcript to search what was said."
        case .indexMissing, .indexing: "Transcript search is still being prepared."
        case .providerUnavailable: "The recall provider is unavailable."
        case .corruptArtifact: "A transcript index needs to be rebuilt."
        case .cancelled: nil
        case .interrupted: "Search was interrupted. Try again."
        case .indexUnavailable, .failed, .unsupported, .queued, .running:
            "Transcript search is unavailable right now."
        case .ready: nil
        }
    }
}
