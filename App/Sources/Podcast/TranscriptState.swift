import Foundation

/// Lifecycle of an episode's transcript ingestion.
///
/// Lane 5 (transcript ingestion) drives transitions; Library UI renders the
/// status capsule (`Downloaded · Transcribing 64%` / `Ready` / etc.). The
/// `source` discriminator lets us label the badge ("Publisher" vs "Scribe").
enum TranscriptState: Codable, Sendable, Hashable {
    /// No work attempted yet.
    case none
    /// Awaiting an upload / publisher-fetch slot.
    case queued
    /// Pulling the publisher's `<podcast:transcript>` payload.
    case fetchingPublisher
    /// Cloud transcription in flight (e.g. ElevenLabs Scribe). Progress in 0...1.
    case transcribing(progress: Double)
    /// Transcript is stored and indexed.
    case ready(source: Source)
    /// Final failure; user can retry. `message` is user-facing.
    case failed(message: String)

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
