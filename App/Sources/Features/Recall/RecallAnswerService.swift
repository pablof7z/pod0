import Foundation

@MainActor
struct RecallAnswerService {
    typealias MetadataResolver = @MainActor (UUID) -> RecallEvidenceMetadata?

    private let rag: any PodcastAgentRAGSearchProtocol
    private let metadata: MetadataResolver
    private let productSignals: any ProductSignalSink

    init(
        rag: any PodcastAgentRAGSearchProtocol,
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared,
        metadata: @escaping MetadataResolver
    ) {
        self.rag = rag
        self.productSignals = productSignals
        self.metadata = metadata
    }

    init(rag: any PodcastAgentRAGSearchProtocol, store: AppStateStore) {
        self.init(rag: rag, productSignals: store.productSignals) { episodeID in
            guard let episode = store.episode(id: episodeID) else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: episode.title,
                podcastTitle: store.podcast(id: episode.podcastID)?.title ?? "Unknown podcast"
            )
        }
    }

    func answer(query: String, limit: Int = 3) async -> RecallAnswer {
        let started = ContinuousClock.now
        record(.init(name: .recallAsked, outcome: .started))
        let answer: RecallAnswer
        do {
            let hits = try await rag.queryTranscripts(
                query: query.trimmed,
                scope: nil,
                limit: max(1, min(limit, 5))
            )
            try Task.checkCancellation()
            let evidence = hits.compactMap(makeEvidence)
            if evidence.isEmpty {
                answer = await emptyAnswer()
            } else {
                answer = RecallAnswer(
                    text: evidence[0].excerpt,
                    evidence: evidence,
                    status: .ready
                )
            }
        } catch is CancellationError {
            answer = RecallAnswer(text: "Recall cancelled.", status: .cancelled)
        } catch {
            answer = RecallAnswer(
                text: "I couldn’t search your transcript evidence right now. Try again in a moment.",
                status: .unavailable
            )
        }
        RecallQualityLogger.outcome(
            status: answer.status,
            evidenceCount: answer.evidence.count,
            duration: ContinuousClock.now - started
        )
        let latency = ProductSignalLatencyBucket.bucket(ContinuousClock.now - started)
        record(.init(
            name: .recallGrounded,
            outcome: Self.signalOutcome(answer.status),
            latencyBucket: latency
        ))
        if !answer.evidence.isEmpty { record(.init(name: .transcriptUsed, outcome: .used)) }
        return answer
    }

    private func emptyAnswer() async -> RecallAnswer {
        switch await rag.transcriptCorpusReadiness() {
        case .ready:
            RecallAnswer(
                text: "I couldn’t find transcript evidence that supports an answer to that question.",
                status: .noEvidence
            )
        case .indexing:
            RecallAnswer(
                text: "Your transcripts are still being indexed. Try this recall again when preparation finishes.",
                status: .indexing
            )
        case .transcriptMissing:
            RecallAnswer(
                text: "No prepared transcripts are available yet. Prepare an episode, then ask again.",
                status: .transcriptMissing
            )
        case .unavailable:
            RecallAnswer(
                text: "Transcript recall is unavailable right now. Check your provider connection and try again.",
                status: .unavailable
            )
        }
    }

    private func makeEvidence(_ hit: TranscriptHit) -> RecallEvidence? {
        guard let chunkID = hit.chunkID.flatMap(UUID.init(uuidString:)),
              let episodeID = UUID(uuidString: hit.episodeID),
              let podcastID = hit.podcastID.flatMap(UUID.init(uuidString:)),
              let artifactVersion = hit.artifactVersion?.nilIfEmpty,
              let provenance = hit.provenance?.nilIfEmpty,
              let resolved = metadata(episodeID)
        else { return nil }
        let excerpt = Self.boundedExcerpt(hit.text)
        guard !excerpt.isEmpty else { return nil }
        let start = Self.milliseconds(hit.startSeconds)
        let end = max(start + 1, Self.milliseconds(hit.endSeconds))
        return RecallEvidence(
            chunkID: chunkID,
            episodeID: episodeID,
            podcastID: podcastID,
            episodeTitle: resolved.episodeTitle,
            podcastTitle: resolved.podcastTitle,
            artifactVersion: artifactVersion,
            startMilliseconds: start,
            endMilliseconds: end,
            excerpt: excerpt,
            provenance: provenance
        )
    }

    private static func boundedExcerpt(_ text: String) -> String {
        let normalized = text.components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        return String(normalized.prefix(420))
    }

    private static func milliseconds(_ seconds: TimeInterval) -> Int64 {
        guard seconds.isFinite else { return 0 }
        return Int64((max(0, seconds) * 1_000).rounded())
    }

    private func record(_ observation: ProductSignalObservation) {
        Task { await productSignals.record(observation) }
    }

    private static func signalOutcome(_ status: RecallAnswer.Status) -> ProductSignalOutcome {
        switch status {
        case .ready: .grounded
        case .cancelled: .cancelled
        case .indexing, .transcriptMissing, .noEvidence, .unavailable: .noEvidence
        }
    }
}
