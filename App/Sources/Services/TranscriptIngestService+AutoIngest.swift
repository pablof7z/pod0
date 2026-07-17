import Foundation

// MARK: - TranscriptIngestService auto-ingest decision logic
//
// Pure desired-state policy used by Reconciler.

extension TranscriptIngestService {

    /// Pure decision logic. Exposed `internal` so
    /// `TranscriptAutoIngestTests` can pin the branching without driving the
    /// full ingest pipeline (which needs network + ElevenLabs + sqlite-vec).
    ///
    /// Inclusion rule:
    ///   - Episode is not already `.ready`.
    ///   - At least one path is available - either the publisher transcript
    ///     URL is present and `autoIngestPublisherTranscripts` is on, OR the
    ///     ElevenLabs key is configured and `autoFallbackToScribe` is on.
    static func autoIngestCandidates(
        among episodes: [Episode],
        settings: Settings,
        elevenLabsKey: String?,
        openRouterKey: String? = nil,
        assemblyAIKey: String? = nil
    ) -> [UUID] {
        let publisherOn = settings.autoIngestPublisherTranscripts
        let sttReady: Bool
        switch settings.sttProvider {
        case .appleNative: sttReady = true   // no API key needed
        case .openRouterWhisper: sttReady = !(openRouterKey ?? "").isEmpty
        case .assemblyAI: sttReady = !(assemblyAIKey ?? "").isEmpty
        case .elevenLabsScribe: sttReady = !(elevenLabsKey ?? "").isEmpty
        }
        let scribeOn = settings.autoFallbackToScribe && sttReady
        guard publisherOn || scribeOn else { return [] }
        return episodes.compactMap { episode -> UUID? in
            guard !Self.isReady(episode.transcriptState) else { return nil }
            if episode.publisherTranscriptURL != nil {
                return publisherOn ? episode.id : nil
            }
            return scribeOn ? episode.id : nil
        }
    }
}
