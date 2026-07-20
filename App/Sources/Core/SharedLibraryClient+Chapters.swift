import Foundation
import Pod0Core

struct SharedChapterCommitResult: Sendable, Equatable {
    let receipt: ChapterCommitReceipt
    let snapshot: SharedChapterSnapshot
}

struct SharedChapterWorkflowReceipt: Codable, Sendable, Equatable {
    static let currentSchemaVersion = 1

    let schemaVersion: Int
    let episodeID: UUID
    let inputVersion: String
    let publisherInputVersion: String?
    let artifactID: String
    let contentDigest: String
    let integrityDigest: String
    let selectionRevision: UInt64

    init(
        summary: ChapterSummaryProjection,
        inputVersion: String,
        publisherInputVersion: String? = nil
    ) throws {
        guard let episodeID = summary.episodeId.uuid else {
            throw SharedLibraryError.unavailable
        }
        self.schemaVersion = Self.currentSchemaVersion
        self.episodeID = episodeID
        self.inputVersion = inputVersion
        self.publisherInputVersion = publisherInputVersion
        self.artifactID = summary.artifactId.stableString
        self.contentDigest = summary.contentDigest.stableString
        self.integrityDigest = summary.integrityDigest.stableString
        self.selectionRevision = summary.selectionRevision.value
    }
}

extension SharedLibraryClient {
    nonisolated func submitChapterObservation(
        _ qualification: ChapterObservationProjection,
        commandID: CommandId = CommandId(uuid: UUID()),
        cancellationID: CancellationId = CancellationId(uuid: UUID()),
        expectedSelectionRevision explicitRevision: StateRevision? = nil
    ) throws -> SharedChapterCommitResult {
        try Task.checkCancellation()
        guard case .qualified(let artifact, _) = qualification,
              let episodeID = artifact.episodeId.uuid
        else { throw SharedLibraryError.invalidChapter }
        let current = try chapterProjection(
            episodeID: episodeID,
            scope: .summary,
            offset: 0,
            maxItems: 1
        )
        let expectedRevision = explicitRevision
            ?? current.summary?.selectionRevision
            ?? StateRevision(value: 0)
        let request = ChapterContractRequest(
            commandId: commandID,
            expectedSelectionRevision: expectedRevision,
            artifact: artifact
        )
        let qualifiedReceipt: ChapterCommitReceipt
        switch projectChapterContract(
            request: request,
            scope: .summary,
            offset: 0,
            maxItems: 1
        ) {
        case .qualified(let receipt, _): qualifiedReceipt = receipt
        case .rejected: throw SharedLibraryError.invalidChapter
        }
        try Task.checkCancellation()
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: cancellationID,
            expectedRevision: nil,
            command: .commitChapter(
                expectedSelectionRevision: expectedRevision,
                artifact: artifact
            )
        ))
        let committed = try chapterProjection(
            episodeID: episodeID,
            scope: .summary,
            offset: 0,
            maxItems: 1
        )
        guard let operation = committed.operations.first(where: {
            $0.commandId == commandID
        }) else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .succeeded: break
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        case .accepted, .running, .blocked:
            throw SharedLibraryError.unavailable
        }
        guard case .chapterCommitted(let receipt) = operation.result,
              receipt.commandId == qualifiedReceipt.commandId,
              receipt.artifactId == qualifiedReceipt.artifactId,
              receipt.contentDigest == qualifiedReceipt.contentDigest,
              receipt.integrityDigest == qualifiedReceipt.integrityDigest,
              receipt.chapterCount == qualifiedReceipt.chapterCount,
              receipt.adSpanCount == qualifiedReceipt.adSpanCount,
              let summary = committed.summary,
              summary.selectionRevision.value >= receipt.selectionRevision.value,
              summary.artifactId == receipt.artifactId,
              let snapshot = try authoritativeChapterReader.load(episodeID: episodeID)
        else { throw SharedLibraryError.unavailable }
        return SharedChapterCommitResult(receipt: receipt, snapshot: snapshot)
    }

    nonisolated func verifyChapterWorkflowReceipt(
        _ receipt: SharedChapterWorkflowReceipt
    ) -> Bool {
        guard receipt.schemaVersion == SharedChapterWorkflowReceipt.currentSchemaVersion,
              let projection = try? chapterProjection(
                episodeID: receipt.episodeID,
                scope: .summary,
                offset: 0,
                maxItems: 1
              ),
              let summary = projection.summary
        else { return false }
        return summary.artifactId.stableString == receipt.artifactID
            && summary.contentDigest.stableString == receipt.contentDigest
            && summary.integrityDigest.stableString == receipt.integrityDigest
            && summary.selectionRevision.value == receipt.selectionRevision
    }

    nonisolated func chapterWorkflowSnapshots(
        episodeIDs: [UUID]
    ) -> [ChapterWorkflowSnapshot] {
        episodeIDs.compactMap { episodeID in
            guard let projection = try? chapterProjection(
                episodeID: episodeID,
                scope: .summary,
                offset: 0,
                maxItems: 1
            ), let summary = projection.summary else { return nil }
            return ChapterWorkflowSnapshot(
                episodeID: episodeID,
                artifactID: summary.artifactId.stableString,
                sourceRevision: summary.sourceRevision,
                contentDigest: summary.contentDigest.stableString,
                selectionRevision: summary.selectionRevision.value,
                provenance: summary.provenance
            )
        }
    }

    nonisolated private func chapterProjection(
        episodeID: UUID,
        scope: ChapterProjectionScope,
        offset: UInt32,
        maxItems: UInt16
    ) throws -> ChapterArtifactProjection {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .chapter(episodeId: EpisodeId(uuid: episodeID), scope: scope),
            offset: offset,
            maxItems: min(max(1, maxItems), SharedChapterReader.maximumPageSize)
        ))
        guard case .chapter(let projection) = envelope.projection else {
            throw SharedLibraryError.unavailable
        }
        if let failure = projection.failure { throw SharedLibraryError(failure.code) }
        return projection
    }
}
