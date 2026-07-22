import Foundation
import Pod0Core

struct SharedTranscriptCommitResult: Sendable, Equatable {
    let receipt: TranscriptCommitReceipt
    let summary: TranscriptSummaryProjection
}

extension SharedLibraryClient {
    nonisolated func submitTranscriptObservation(
        _ transcript: Transcript,
        context: TranscriptObservationContext,
        commandID: CommandId = CommandId(uuid: UUID()),
        cancellationID: CancellationId = CancellationId(uuid: UUID()),
        expectedSelectionRevision explicitRevision: StateRevision? = nil
    ) throws -> SharedTranscriptCommitResult {
        try Task.checkCancellation()
        let artifact = try TranscriptObservationMapper.map(transcript, context: context)
        let currentProjection = try transcriptProjection(
            episodeID: transcript.episodeID,
            scope: .summary,
            offset: 0,
            maxItems: 1
        )
        let expectedRevision = explicitRevision
            ?? currentProjection.summary?.selectionRevision
            ?? StateRevision(value: 0)
        let request = TranscriptCommitRequest(
            commandId: commandID,
            expectedSelectionRevision: expectedRevision,
            artifact: artifact
        )
        let qualifiedReceipt: TranscriptCommitReceipt
        switch projectTranscriptContract(
            request: request,
            scope: .summary,
            offset: 0,
            maxItems: 1
        ) {
        case .qualified(let receipt, _): qualifiedReceipt = receipt
        case .rejected: throw SharedLibraryError.invalidTranscript
        }
        try Task.checkCancellation()
        facade.dispatch(command: CommandEnvelope(
            commandId: commandID,
            cancellationId: cancellationID,
            expectedRevision: nil,
            command: .commitTranscript(
                expectedSelectionRevision: expectedRevision,
                artifact: artifact
            )
        ))
        let committedProjection = try transcriptProjection(
            episodeID: transcript.episodeID,
            scope: .summary,
            offset: 0,
            maxItems: 1
        )
        guard let operation = committedProjection.operations.first(where: {
            $0.commandId == commandID
        }) else { throw SharedLibraryError.unavailable }
        switch operation.stage {
        case .succeeded: break
        case .failed, .cancelled, .unsupported:
            throw SharedLibraryError(operation.failure?.code)
        case .accepted, .running, .blocked:
            throw SharedLibraryError.unavailable
        }
        guard case .transcriptCommitted(let receipt) = operation.result,
              receipt.commandId == qualifiedReceipt.commandId,
              receipt.artifactId == qualifiedReceipt.artifactId,
              receipt.transcriptVersionId == qualifiedReceipt.transcriptVersionId,
              receipt.transcriptContentDigest == qualifiedReceipt.transcriptContentDigest,
              receipt.artifactIntegrityDigest == qualifiedReceipt.artifactIntegrityDigest,
              receipt.speakerCount == qualifiedReceipt.speakerCount,
              receipt.segmentCount == qualifiedReceipt.segmentCount,
              receipt.wordCount == qualifiedReceipt.wordCount,
              let summary = committedProjection.summary,
              summary.selectionRevision.value >= receipt.selectionRevision.value,
              summary.artifactId == receipt.artifactId
        else { throw SharedLibraryError.unavailable }
        return SharedTranscriptCommitResult(receipt: receipt, summary: summary)
    }

    nonisolated func transcriptWorkflowSnapshots(
        episodeIDs: [UUID]
    ) -> [TranscriptWorkflowSnapshot] {
        episodeIDs.compactMap { episodeID in
            guard let projection = try? transcriptProjection(
                episodeID: episodeID,
                scope: .summary,
                offset: 0,
                maxItems: 1
            ), let summary = projection.summary else { return nil }
            return TranscriptWorkflowSnapshot(
                episodeID: episodeID,
                sourceRevision: summary.sourceRevision,
                contentDigest: summary.transcriptContentDigest.stableString,
                selectionRevision: summary.selectionRevision.value
            )
        }
    }

    nonisolated private func transcriptProjection(
        episodeID: UUID,
        scope: TranscriptProjectionScope,
        offset: UInt32,
        maxItems: UInt16
    ) throws -> TranscriptProjection {
        let envelope = facade.snapshot(request: ProjectionRequest(
            scope: .transcript(
                episodeId: EpisodeId(uuid: episodeID),
                scope: scope
            ),
            offset: offset,
            maxItems: min(max(1, maxItems), SharedTranscriptReader.maximumPageSize)
        ))
        guard case .transcript(let projection) = envelope.projection else {
            throw SharedLibraryError.unavailable
        }
        if let failure = projection.failure { throw SharedLibraryError(failure.code) }
        return projection
    }
}
