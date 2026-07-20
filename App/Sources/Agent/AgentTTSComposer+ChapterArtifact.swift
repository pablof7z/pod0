import Foundation
import Pod0Core

extension AgentTTSComposer {
    /// Commits the agent's raw composition through Rust qualification. Native
    /// constructs the platform-produced observation; Rust owns validation,
    /// identifiers, provenance, persistence, and the selected projection.
    @MainActor
    func commitGeneratedChapters(
        _ chapters: [Episode.Chapter],
        durationSeconds: TimeInterval,
        for episode: Episode
    ) async throws {
        guard let store, let sharedLibrary = store.sharedLibrary else {
            throw AgentTTSError.storeUnavailable
        }
        let items = chapters.enumerated().map { index, chapter in
            let nextStart = chapters.indices.contains(index + 1)
                ? chapters[index + 1].startTime
                : durationSeconds
            return AgentComposedChapterItem(
                startSeconds: chapter.startTime,
                endSeconds: chapter.endTime ?? nextStart,
                title: chapter.title,
                summary: chapter.summary,
                imageUrl: chapter.imageURL?.absoluteString,
                linkUrl: chapter.linkURL?.absoluteString,
                includeInTableOfContents: chapter.includeInTableOfContents,
                sourceEpisodeId: chapter.sourceEpisodeID
                    .flatMap(UUID.init(uuidString:))
                    .map(EpisodeId.init(uuid:))
            )
        }
        let payload = items.map(AgentChapterDigestItem.init)
        let encoder = JSONEncoder()
        encoder.outputFormatting = [.sortedKeys]
        let digestString = ArtifactRepository.hash(try encoder.encode(payload))
        guard let digest = ContentDigest(hexadecimal: digestString) else {
            throw SharedLibraryError.invalidChapter
        }
        let requestID = HostRequestId(uuid: UUID())
        let cancellationID = CancellationId(uuid: UUID())
        let adapter = ChapterObservationCapabilityAdapter()
        let response = await adapter.execute(ChapterCapabilityRequestEnvelope(
            requestID: requestID,
            cancellationID: cancellationID,
            request: .agent(.init(
                episodeID: EpisodeId(uuid: episode.id),
                podcastID: PodcastId(uuid: episode.podcastID),
                compositionRevision: "agent-composition-v1:\(digest.stableString)",
                policyVersion: 1,
                provider: "pod0AgentComposer",
                model: nil,
                sourcePayloadDigest: digest,
                generatedAt: UnixTimestampMilliseconds(date: Date()),
                durationMilliseconds: UInt64((durationSeconds * 1_000).rounded()),
                items: items
            ))
        ))
        switch response.outcome {
        case .failed(let failure):
            if failure.code == .cancelled { throw CancellationError() }
            throw SharedLibraryError.invalidChapter
        case .observed(_, _, let qualification):
            _ = try sharedLibrary.submitChapterObservation(
                qualification,
                cancellationID: cancellationID
            )
        }
    }
}

private struct AgentChapterDigestItem: Codable {
    let start: Double
    let end: Double
    let title: String
    let summary: String?
    let imageURL: String?
    let linkURL: String?
    let includeInTableOfContents: Bool
    let sourceEpisodeID: String?

    init(_ item: AgentComposedChapterItem) {
        start = item.startSeconds
        end = item.endSeconds
        title = item.title
        summary = item.summary
        imageURL = item.imageUrl
        linkURL = item.linkUrl
        includeInTableOfContents = item.includeInTableOfContents
        sourceEpisodeID = item.sourceEpisodeId?.uuid?.uuidString
    }
}
