import Foundation
import Pod0Core
import os.log

struct SharedTranscriptShadowResult: Sendable, Equatable {
    let receipt: TranscriptCommitReceipt
    let mismatches: Set<TranscriptShadowMismatch>
}

private let sharedTranscriptShadowLogger = Logger.app("SharedTranscriptShadow")

extension SharedLibraryClient {
    nonisolated func submitTranscriptObservation(
        _ transcript: Transcript,
        context: TranscriptObservationContext,
        expectedSelectionRevision explicitRevision: StateRevision? = nil
    ) throws -> SharedTranscriptShadowResult {
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
        let commandID = CommandId(uuid: UUID())
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
            cancellationId: CancellationId(uuid: UUID()),
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
              receipt == qualifiedReceipt,
              committedProjection.summary?.selectionRevision == receipt.selectionRevision,
              let candidate = try SharedTranscriptReader(facade: facade)
                .loadThrowing(episodeID: transcript.episodeID),
              let summary = committedProjection.summary
        else { throw SharedLibraryError.unavailable }

        let mismatches = SharedTranscriptShadowComparator.compare(
            authoritative: transcript,
            podcastID: context.podcastID,
            context: context,
            summary: summary,
            candidate: candidate
        )
        logShadowResult(
            episodeID: transcript.episodeID,
            receipt: receipt,
            mismatches: mismatches
        )
        return SharedTranscriptShadowResult(receipt: receipt, mismatches: mismatches)
    }

    nonisolated func submitTranscriptObservationOffMain(
        _ transcript: Transcript,
        context: TranscriptObservationContext
    ) async throws -> SharedTranscriptShadowResult {
        await Task.yield()
        return try submitTranscriptObservation(transcript, context: context)
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

    nonisolated private func logShadowResult(
        episodeID: UUID,
        receipt: TranscriptCommitReceipt,
        mismatches: Set<TranscriptShadowMismatch>
    ) {
        let categories = mismatches.map(\.rawValue).sorted().joined(separator: ",")
        if mismatches.isEmpty {
            sharedTranscriptShadowLogger.debug(
                "transcript shadow matched episode=\(episodeID, privacy: .public) artifact=\(receipt.artifactId.stableString, privacy: .public) digest=\(receipt.transcriptContentDigest.stableString, privacy: .public)"
            )
        } else {
            sharedTranscriptShadowLogger.notice(
                "transcript shadow mismatch episode=\(episodeID, privacy: .public) artifact=\(receipt.artifactId.stableString, privacy: .public) digest=\(receipt.transcriptContentDigest.stableString, privacy: .public) categories=\(categories, privacy: .public)"
            )
        }
    }
}

@MainActor
enum SharedTranscriptShadowObserver {
    private static let logger = Logger.app("SharedTranscriptShadow")

    static func observe(
        transcript: Transcript,
        podcastID: UUID,
        sourceRevision: String,
        sourcePayloadDigest: String,
        provider: String?,
        client: SharedLibraryClient?
    ) async {
        guard let client else {
            logger.notice("transcript shadow core unavailable; Swift authority retained")
            return
        }
        do {
            _ = try await client.submitTranscriptObservationOffMain(
                transcript,
                context: TranscriptObservationContext(
                    podcastID: podcastID,
                    sourceRevision: sourceRevision,
                    sourcePayloadDigest: sourcePayloadDigest,
                    provider: provider
                )
            )
        } catch is CancellationError {
            logger.notice("transcript shadow commit cancelled before dispatch")
        } catch {
            logger.notice("transcript shadow commit deferred; Swift authority retained")
        }
    }
}
