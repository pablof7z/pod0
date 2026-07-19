import Foundation

/// Native transcript read boundary backed by bounded shared-core projections.
protocol TranscriptReading: Sendable {
    func load(episodeID: UUID) -> Transcript?
}

struct UnavailableTranscriptReader: TranscriptReading {
    static let shared = UnavailableTranscriptReader()

    func load(episodeID: UUID) -> Transcript? {
        _ = episodeID
        return nil
    }
}
