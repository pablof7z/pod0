import Foundation

/// Migration-safe transcript read boundary.
///
/// Swift JSON remains authoritative through #96. Issue #97 replaces the
/// default implementation with shared-core projections and deletes the
/// legacy durable reader.
protocol TranscriptReading: Sendable {
    func load(episodeID: UUID) -> Transcript?
}

extension TranscriptStore: TranscriptReading {}
