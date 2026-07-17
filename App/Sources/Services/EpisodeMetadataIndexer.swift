import Foundation

/// Produces the title/description vector artifact for one coordinator-owned
/// job. Artifact currency and retry state live outside `Episode`.
@MainActor
final class EpisodeMetadataIndexer {
    static let shared = EpisodeMetadataIndexer()

    private let store: VectorStore
    private var inFlight: Set<UUID> = []

    init(store: VectorStore = RAGService.shared.index) {
        self.store = store
    }

    func indexEpisode(id: UUID, appStore: AppStateStore) async throws {
        guard !inFlight.contains(id) else {
            throw JobFailure(
                classification: .transient,
                message: "Metadata indexing is already running."
            )
        }
        guard let episode = appStore.episode(id: id) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        guard let chunk = Self.makeChunk(for: episode) else { return }
        inFlight.insert(id)
        defer { inFlight.remove(id) }
        try await store.upsert(chunks: [chunk])
    }

    func indexEpisode(
        id: UUID,
        appStore: AppStateStore,
        generation: String
    ) async throws -> VectorArtifactReceipt {
        guard !inFlight.contains(id) else {
            throw JobFailure(
                classification: .transient,
                message: "Metadata indexing is already running."
            )
        }
        guard let episode = appStore.episode(id: id) else {
            throw JobFailure(classification: .invalidInput, message: "Episode no longer exists")
        }
        let chunks = Self.makeChunk(for: episode).map { [$0] } ?? []
        inFlight.insert(id)
        defer { inFlight.remove(id) }
        guard let vectorIndex = store as? VectorIndex else {
            if !chunks.isEmpty { try await store.upsert(chunks: chunks) }
            return VectorArtifactReceipt(
                generation: generation,
                artifactKind: VectorIndex.metadataArtifactKind,
                chunkCount: chunks.count,
                schemaVersion: VectorIndex.artifactSchemaVersion
            )
        }
        return try await vectorIndex.stageArtifact(
            chunks: chunks,
            episodeID: id,
            generation: generation,
            artifactKind: VectorIndex.metadataArtifactKind
        )
    }

    private static func makeChunk(for episode: Episode) -> Chunk? {
        let title = episode.title.trimmingCharacters(in: .whitespacesAndNewlines)
        let description = EpisodeShowNotesFormatter.plainText(from: episode.description)
            .trimmingCharacters(in: .whitespacesAndNewlines)
        let text = [title, description].filter { !$0.isEmpty }.joined(separator: "\n\n")
        guard !text.isEmpty else { return nil }
        return Chunk(
            episodeID: episode.id,
            podcastID: episode.podcastID,
            text: text,
            startMS: 0,
            endMS: 0,
            speakerID: nil
        )
    }
}
