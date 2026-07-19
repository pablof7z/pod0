import Foundation
import Pod0Core
import os.log

struct SharedEvidenceReceipt: Codable, Sendable, Equatable {
    static let schemaVersion = 1

    let episodeID: UUID
    let inputVersion: String
    let generationID: String
    let transcriptContentDigest: String
    let spanCount: UInt32
    let schemaVersion: Int
}

extension SharedLibraryClient {
    private static let evidenceLogger = Logger.app("SharedEvidenceRebuild")

    func attachRecall(_ capabilities: RecallCapabilityService, store: AppStateStore) {
        guard !recallHostAttached else { return }
        recallHostAttached = true
        let host = CoreRecallHost(
            projections: facade,
            index: capabilities.index,
            embedder: capabilities.embedder,
            reranker: OpenRouterRerankerClient(),
            isRerankingEnabled: { [weak store] in
                await MainActor.run { store?.state.settings.rerankerEnabled ?? false }
            }
        )
        evidenceRebuildTask = Task { @MainActor [weak self, weak store] in
            guard let self else { return }
            await deferredRecallHost.attach(host)
            guard !Task.isCancelled, let store else { return }
            await rebuildExistingEvidence(in: store)
        }
    }

    func rebuildTranscriptEvidence(
        transcript: Transcript,
        summary: TranscriptSummaryProjection,
        inputVersion: String = "startup-rebuild"
    ) async throws -> SharedEvidenceReceipt {
        guard summary.episodeId.uuid == transcript.episodeID else {
            throw SharedLibraryError.unavailable
        }
        guard rebuildingEvidenceEpisodeIDs.insert(transcript.episodeID).inserted else {
            throw SharedLibraryError.unavailable
        }
        defer { rebuildingEvidenceEpisodeIDs.remove(transcript.episodeID) }
        let segments = try transcript.segments.map { segment in
            TranscriptSegmentInput(
                text: segment.text,
                startMilliseconds: try Self.milliseconds(segment.start),
                endMilliseconds: try Self.milliseconds(segment.end),
                speakerId: segment.speakerID.map(SpeakerId.init(uuid:))
            )
        }
        let result = try await execute(.rebuildTranscriptEvidence(
            input: TranscriptEvidenceInput(
                episodeId: EpisodeId(uuid: transcript.episodeID),
                podcastId: summary.podcastId,
                sourceRevision: summary.sourceRevision,
                source: summary.source,
                provider: summary.provider,
                sourcePayloadDigest: summary.sourcePayloadDigest,
                segments: segments
            ),
            policy: EvidenceChunkPolicy(
                version: 1,
                targetTokens: 400,
                overlapPerMille: 150,
                snapTolerancePerMille: 200
            )
        ))
        guard case .evidenceRebuilt(let episodeID, let generationID, let spanCount) = result,
              episodeID.uuid == transcript.episodeID,
              let projection = evidenceIndex(episodeID: episodeID),
              projection.stage == .ready,
              projection.generationId == generationID,
              projection.totalSpans == spanCount,
              let digest = projection.transcriptContentDigest,
              digest == summary.transcriptContentDigest else {
            throw SharedLibraryError.unavailable
        }
        return SharedEvidenceReceipt(
            episodeID: transcript.episodeID,
            inputVersion: inputVersion,
            generationID: generationID.stableString,
            transcriptContentDigest: digest.stableString,
            spanCount: spanCount,
            schemaVersion: SharedEvidenceReceipt.schemaVersion
        )
    }

    func verifyEvidenceReceipt(_ receipt: SharedEvidenceReceipt) -> Bool {
        let episodeID = EpisodeId(uuid: receipt.episodeID)
        guard receipt.schemaVersion == SharedEvidenceReceipt.schemaVersion,
              let projection = evidenceIndex(episodeID: episodeID),
              projection.stage == .ready,
              projection.generationId?.stableString == receipt.generationID,
              projection.transcriptContentDigest?.stableString
                == receipt.transcriptContentDigest,
              projection.totalSpans == receipt.spanCount else { return false }
        return true
    }

    private func rebuildExistingEvidence(in store: AppStateStore) async {
        let episodes = store.state.episodes
            .filter { if case .ready = $0.transcriptState { true } else { false } }
            .sorted { $0.id.uuidString < $1.id.uuidString }
        for episode in episodes {
            guard !Task.isCancelled,
                  let transcript = authoritativeTranscriptReader.load(episodeID: episode.id),
                  let summary = try? authoritativeTranscriptReader.summary(
                    episodeID: episode.id
                  ) else {
                continue
            }
            do {
                _ = try await rebuildTranscriptEvidence(
                    transcript: transcript,
                    summary: summary
                )
            } catch is CancellationError {
                return
            } catch {
                Self.evidenceLogger.notice(
                    "recall evidence rebuild deferred for one episode"
                )
            }
        }
    }

    private static func milliseconds(_ seconds: TimeInterval) throws -> UInt64 {
        let value = seconds * 1_000
        guard value.isFinite, value >= 0, value <= Double(UInt64.max) else {
            throw SharedLibraryError.unavailable
        }
        return UInt64(value.rounded())
    }

    private func evidenceIndex(episodeID: EpisodeId) -> EvidenceIndexProjection? {
        guard case .evidenceIndex(let projection) = facade.snapshot(request: ProjectionRequest(
            scope: .evidenceIndex(episodeId: episodeID),
            offset: 0,
            maxItems: 1
        )).projection else { return nil }
        return projection
    }
}
