import Foundation
import Pod0Core

@MainActor
struct RecallAnswerService {
    typealias MetadataResolver = @MainActor (UUID) -> RecallEvidenceMetadata?
    typealias Search = @MainActor @Sendable (
        String,
        RecallScope,
        UInt16
    ) async -> RecallResultProjection

    private let search: Search
    private let metadata: MetadataResolver
    private let productSignals: any ProductSignalSink

    init(
        search: @escaping Search,
        productSignals: any ProductSignalSink = DiscardingProductSignalSink.shared,
        metadata: @escaping MetadataResolver
    ) {
        self.search = search
        self.productSignals = productSignals
        self.metadata = metadata
    }

    init?(store: AppStateStore) {
        guard let client = store.sharedLibrary else { return nil }
        self.init(
            search: { [weak client] query, scope, limit in
                guard let client else {
                    return RecallResultProjection.interrupted()
                }
                return await client.recall(query: query, scope: scope, limit: limit)
            },
            productSignals: store.productSignals
        ) { episodeID in
            guard let episode = store.episode(id: episodeID) else { return nil }
            return RecallEvidenceMetadata(
                episodeTitle: episode.title,
                podcastTitle: store.podcast(id: episode.podcastID)?.title ?? "Unknown podcast"
            )
        }
    }

    func answer(query: String, limit: UInt16 = 3) async -> RecallAnswer {
        let started = ContinuousClock.now
        record(.init(name: .recallAsked, outcome: .started))
        let projection = await search(query.trimmed, .library, limit)
        let answer = makeAnswer(projection)
        RecallQualityLogger.outcome(
            status: answer.status,
            evidenceCount: answer.evidence.count,
            duration: ContinuousClock.now - started
        )
        record(.init(
            name: .recallGrounded,
            outcome: Self.signalOutcome(answer.status),
            latencyBucket: ProductSignalLatencyBucket.bucket(ContinuousClock.now - started)
        ))
        if !answer.evidence.isEmpty {
            record(.init(name: .transcriptUsed, outcome: .used))
        }
        return answer
    }

    private func makeAnswer(_ projection: RecallResultProjection) -> RecallAnswer {
        switch projection.stage {
        case .ready:
            let evidence = projection.evidence.compactMap(makeEvidence)
            guard evidence.count == projection.evidence.count, let first = evidence.first else {
                return unavailable("Transcript evidence could not be displayed safely.")
            }
            return RecallAnswer(text: first.excerpt, evidence: evidence, status: .ready)
        case .noEvidence:
            return RecallAnswer(
                text: "I couldn’t find transcript evidence that supports an answer to that question.",
                status: .noEvidence
            )
        case .transcriptMissing:
            return RecallAnswer(
                text: "No prepared transcripts are available yet. Prepare an episode, then ask again.",
                status: .transcriptMissing
            )
        case .indexMissing:
            return RecallAnswer(
                text: "A transcript is ready, but its recall index still needs to be prepared.",
                status: .indexMissing
            )
        case .indexing:
            return RecallAnswer(
                text: "Your transcripts are still being indexed. Try this recall again when preparation finishes.",
                status: .indexing
            )
        case .indexUnavailable:
            return RecallAnswer(
                text: "The transcript index is unavailable right now. Reopen Pod0 and try again.",
                status: .indexUnavailable
            )
        case .providerUnavailable:
            return RecallAnswer(
                text: "Your recall provider is unavailable. Check its connection and try again.",
                status: .providerUnavailable
            )
        case .corruptArtifact:
            return RecallAnswer(
                text: "One transcript index needs to be rebuilt before it can support recall.",
                status: .corruptArtifact
            )
        case .interrupted:
            return RecallAnswer(
                text: "That recall was interrupted when Pod0 restarted. Ask again to resume safely.",
                status: .interrupted
            )
        case .cancelled:
            return RecallAnswer(text: "Recall cancelled.", status: .cancelled)
        case .queued, .running, .failed, .unsupported:
            return unavailable("I couldn’t search your transcript evidence right now. Try again in a moment.")
        }
    }

    private func makeEvidence(_ item: RecallEvidenceProjection) -> RecallEvidence? {
        guard let episodeID = item.episodeId.uuid,
              let podcastID = item.podcastId.uuid,
              let resolved = metadata(episodeID),
              !item.excerpt.isEmpty,
              item.endMilliseconds > item.startMilliseconds else { return nil }
        return RecallEvidence(
            spanID: item.spanId.stableString,
            episodeID: episodeID,
            podcastID: podcastID,
            episodeTitle: resolved.episodeTitle,
            podcastTitle: resolved.podcastTitle,
            generationID: item.generationId.stableString,
            transcriptVersionID: item.transcriptVersionId.stableString,
            transcriptContentDigest: item.transcriptContentDigest.stableString,
            firstSegmentID: item.firstSegmentId.stableString,
            lastSegmentID: item.lastSegmentId.stableString,
            startSegmentOrdinal: item.startSegmentOrdinal,
            endSegmentOrdinalExclusive: item.endSegmentOrdinalExclusive,
            startMilliseconds: item.startMilliseconds,
            endMilliseconds: item.endMilliseconds,
            excerpt: item.excerpt,
            speakerID: item.speakerId?.stableString,
            provenance: RecallEvidenceProvenance(
                source: item.provenance.source.stableName,
                provider: item.provenance.provider,
                sourcePayloadDigest: item.provenance.sourcePayloadDigest.stableString
            ),
            score: RecallEvidenceScore(
                vectorRRFUnits: item.score.vectorRrfUnits,
                lexicalRRFUnits: item.score.lexicalRrfUnits,
                totalRRFUnits: item.score.totalRrfUnits,
                baseRank: item.score.baseRank,
                rerankRank: item.score.rerankRank
            )
        )
    }

    private func unavailable(_ text: String) -> RecallAnswer {
        RecallAnswer(text: text, status: .unavailable)
    }

    private func record(_ observation: ProductSignalObservation) {
        Task { await productSignals.record(observation) }
    }

    private static func signalOutcome(_ status: RecallAnswer.Status) -> ProductSignalOutcome {
        switch status {
        case .ready: .grounded
        case .cancelled: .cancelled
        case .indexing, .transcriptMissing, .indexMissing, .noEvidence: .noEvidence
        case .indexUnavailable, .providerUnavailable, .corruptArtifact, .interrupted, .unavailable:
            .failed
        }
    }
}

extension Pod0Core.TranscriptSource {
    var stableName: String {
        switch self {
        case .publisher: "publisher"
        case .scribe: "scribe"
        case .whisper: "whisper"
        case .onDevice: "onDevice"
        case .assemblyAi: "assemblyAI"
        case .other: "other"
        case .unsupported(let wireCode): "unsupported:\(wireCode)"
        }
    }
}
