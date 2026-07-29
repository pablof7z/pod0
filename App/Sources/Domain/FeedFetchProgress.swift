import Foundation

/// Bounded read model of one durable Rust feed-fetch workflow
/// (`LibraryProjection.feedFetches`). Subscribing commits immediately in
/// Rust; this projection is what lets rows render "Subscribing…" from state
/// that survives relaunch instead of per-row native spinner state.
struct FeedFetchProgress: Equatable, Sendable {
    enum Stage: Equatable, Sendable {
        /// The fetch is queued or in flight.
        case fetching
        /// A transient failure occurred; Rust scheduled a durable retry.
        case retryScheduled
        /// The workflow parked after a terminal failure.
        case failed
    }

    var podcastID: UUID?
    var feedURLString: String
    var stage: Stage
    var attempt: UInt16
    var failureCode: String?

    var isActive: Bool { stage != .failed }
}
