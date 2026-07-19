import CryptoKit
import Foundation
import Pod0Core
import os.log

extension SharedLibraryClient {
    private static let evidenceLogger = Logger.app("SharedEvidenceRebuild")

    func attachRecall(_ rag: RAGService, store: AppStateStore) {
        guard !recallHostAttached else { return }
        recallHostAttached = true
        let host = CoreRecallHost(
            projections: facade,
            index: rag.index,
            embedder: rag.embedder,
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
        podcastID: UUID,
        selectedData: Data
    ) async throws -> OperationResult? {
        guard rebuildingEvidenceEpisodeIDs.insert(transcript.episodeID).inserted else { return nil }
        defer { rebuildingEvidenceEpisodeIDs.remove(transcript.episodeID) }
        let digestBytes = Array(SHA256.hash(data: selectedData))
        let digestHex = digestBytes.map { String(format: "%02x", $0) }.joined()
        let segments = try transcript.segments.map { segment in
            TranscriptSegmentInput(
                text: segment.text,
                startMilliseconds: try Self.milliseconds(segment.start),
                endMilliseconds: try Self.milliseconds(segment.end),
                speakerId: segment.speakerID.map(SpeakerId.init(uuid:))
            )
        }
        return try await execute(.rebuildTranscriptEvidence(
            input: TranscriptEvidenceInput(
                episodeId: EpisodeId(uuid: transcript.episodeID),
                podcastId: PodcastId(uuid: podcastID),
                sourceRevision: "selected-json-sha256:\(digestHex)",
                source: Self.coreSource(transcript.source),
                provider: nil,
                sourcePayloadDigest: ContentDigest(
                    word0: Self.digestWord(digestBytes, at: 0),
                    word1: Self.digestWord(digestBytes, at: 8),
                    word2: Self.digestWord(digestBytes, at: 16),
                    word3: Self.digestWord(digestBytes, at: 24)
                ),
                segments: segments
            ),
            policy: EvidenceChunkPolicy(
                version: 1,
                targetTokens: 400,
                overlapPerMille: 150,
                snapTolerancePerMille: 200
            )
        ))
    }

    func scheduleTranscriptEvidenceRebuild(
        transcript: Transcript,
        podcastID: UUID,
        selectedData: Data
    ) {
        guard evidenceUpdateTasks[transcript.episodeID] == nil else { return }
        let episodeID = transcript.episodeID
        evidenceUpdateTasks[episodeID] = Task { @MainActor [weak self] in
            defer { self?.evidenceUpdateTasks.removeValue(forKey: episodeID) }
            do {
                _ = try await self?.rebuildTranscriptEvidence(
                    transcript: transcript,
                    podcastID: podcastID,
                    selectedData: selectedData
                )
            } catch is CancellationError {
                return
            } catch {
                Self.evidenceLogger.notice(
                    "recall evidence update deferred for one episode"
                )
            }
        }
    }

    private func rebuildExistingEvidence(in store: AppStateStore) async {
        let transcriptStore = TranscriptStore.shared
        let episodes = store.state.episodes
            .filter { if case .ready = $0.transcriptState { true } else { false } }
            .sorted { $0.id.uuidString < $1.id.uuidString }
        for episode in episodes {
            guard !Task.isCancelled,
                  let transcript = transcriptStore.load(episodeID: episode.id),
                  let data = transcriptStore.verifiedData(episodeID: episode.id) else { continue }
            do {
                _ = try await rebuildTranscriptEvidence(
                    transcript: transcript,
                    podcastID: episode.podcastID,
                    selectedData: data
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

    private static func coreSource(_ source: TranscriptSource) -> Pod0Core.TranscriptSource {
        switch source {
        case .publisher: .publisher
        case .scribeV1: .scribe
        case .whisper: .whisper
        case .onDevice: .onDevice
        case .assemblyAI: .assemblyAi
        }
    }

    private static func digestWord(_ bytes: [UInt8], at offset: Int) -> UInt64 {
        bytes[offset..<(offset + 8)].reduce(0) { ($0 << 8) | UInt64($1) }
    }
}
