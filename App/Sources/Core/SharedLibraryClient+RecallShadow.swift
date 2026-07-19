import Foundation
import Pod0Core

extension SharedLibraryClient {
    func scheduleRecallShadow(
        query: String,
        legacyEvidence: [RecallEvidence],
        limit: Int
    ) {
        let taskID = UUID()
        recallShadowTasks[taskID] = Task { @MainActor [weak self] in
            defer { self?.recallShadowTasks.removeValue(forKey: taskID) }
            guard let self else { return }
            let started = ContinuousClock.now
            let outcome: ProductSignalOutcome
            do {
                let queryID = RecallQueryId(uuid: UUID())
                _ = try await execute(.recallQuery(query: RecallQuery(
                    queryId: queryID,
                    text: query,
                    scope: .library,
                    limit: UInt16(max(1, min(limit, 20)))
                )))
                try Task.checkCancellation()
                guard case .recall(let projection) = facade.snapshot(
                    request: ProjectionRequest(
                        scope: .recall(queryId: queryID),
                        offset: 0,
                        maxItems: UInt16(max(1, min(limit, 20)))
                    )
                ).projection else { return }
                outcome = RecallShadowParity.matches(
                    legacy: legacyEvidence,
                    shared: projection.evidence
                ) ? .matched : .mismatched
            } catch is CancellationError {
                return
            } catch {
                outcome = .failed
            }
            await store?.productSignals.record(ProductSignalObservation(
                name: .recallShadowParity,
                outcome: outcome,
                latencyBucket: ProductSignalLatencyBucket.bucket(
                    ContinuousClock.now - started
                )
            ))
        }
    }
}

enum RecallShadowParity {
    static func matches(
        legacy: [RecallEvidence],
        shared: [RecallEvidenceProjection]
    ) -> Bool {
        keys(legacy) == keys(shared)
    }

    private struct Key: Equatable {
        let episodeID: UUID
        let startMilliseconds: Int64
        let endMilliseconds: Int64
        let excerpt: String
    }

    private static func keys(_ evidence: [RecallEvidence]) -> [Key]? {
        evidence.map {
            Key(
                episodeID: $0.episodeID,
                startMilliseconds: $0.startMilliseconds,
                endMilliseconds: $0.endMilliseconds,
                excerpt: boundedExcerpt($0.excerpt)
            )
        }
    }

    private static func keys(_ evidence: [RecallEvidenceProjection]) -> [Key]? {
        var keys: [Key] = []
        for item in evidence {
            guard let episodeID = item.episodeId.uuid,
                  let start = Int64(exactly: item.startMilliseconds),
                  let end = Int64(exactly: item.endMilliseconds) else { return nil }
            keys.append(Key(
                episodeID: episodeID,
                startMilliseconds: start,
                endMilliseconds: end,
                excerpt: boundedExcerpt(item.excerpt)
            ))
        }
        return keys
    }

    private static func boundedExcerpt(_ text: String) -> String {
        let normalized = text.components(separatedBy: .whitespacesAndNewlines)
            .filter { !$0.isEmpty }
            .joined(separator: " ")
        return String(normalized.prefix(420))
    }
}
