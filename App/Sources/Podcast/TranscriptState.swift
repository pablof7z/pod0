import Foundation

/// Stable transcript artifact projection. Execution lifecycle is projected
/// from JobStore rather than persisted a second time on Episode.
enum TranscriptState: Codable, Sendable, Hashable {
    /// No verified current transcript artifact.
    case none
    /// Transcript is stored and readable. Semantic indexing is a separate artifact.
    case ready(source: Source)

    /// Where the resolved transcript came from.
    enum Source: String, Codable, Sendable, Hashable {
        case publisher
        case scribe
        case whisper
        case onDevice
        case assemblyAI
        case other
    }
}
