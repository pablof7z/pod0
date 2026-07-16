// The current exact NMP pin does not yet ship the typed module. The second
// condition prevents a generic NMP package from accidentally compiling this
// future adapter against raw APIs. Enable only with the typed-module pin.
#if canImport(NMP) && POD0_NMP_TYPED_NIP22
import Foundation
import NMP

/// Production comments adapter. Its only dependency is the app's one
/// long-lived NMP composition; no relay, signer, event, or retry owner is
/// created here.
struct NMPEpisodeCommentsRepository: EpisodeCommentsRepository {
    private let access: any Pod0NMPEngineAccess

    init(access: any Pod0NMPEngineAccess) {
        self.access = access
    }

    func activeAuthorPubkey() async throws -> String? {
        try access.engine.activeAccount()
    }

    func observe(target: CommentTarget) async throws -> EpisodeCommentObservation {
        let nmpTarget = try makeTarget(target)
        let query = try access.engine.observeEpisodeComments(nmpTarget)
        let (updates, continuation) = AsyncThrowingStream.makeStream(
            of: EpisodeCommentSnapshot.self
        )
        let task = Task {
            for await batch in query {
                guard !Task.isCancelled else { break }
                continuation.yield(map(batch, target: target))
            }
            continuation.finish()
        }
        return EpisodeCommentObservation(updates: updates) {
            task.cancel()
            query.cancel()
            continuation.finish()
        }
    }

    func publish(content: String, target: CommentTarget) async throws -> EpisodeCommentReceipt {
        let intent = try access.engine.episodeCommentIntent(
            target: makeTarget(target),
            content: content
        )
        return bridge(try await access.engine.publishComposed(intent))
    }

    func reattachReceipt(id: UInt64) async throws -> EpisodeCommentReceiptReattachment {
        switch try access.engine.reattachReceipt(id: id) {
        case .attached(let receipt): return .attached(bridge(receipt))
        case .notFound: return .notFound
        case .retainedButUnreadable: return .retainedButUnreadable
        }
    }

    private func makeTarget(_ target: CommentTarget) throws -> PodcastEpisodeCommentTarget {
        switch target {
        case .episode(let guid): return try PodcastEpisodeCommentTarget(guid: guid)
        }
    }

    private func map(
        _ batch: NMP.EpisodeCommentBatch,
        target: CommentTarget
    ) -> EpisodeCommentSnapshot {
        EpisodeCommentSnapshot(
            comments: batch.comments.map {
                EpisodeComment(
                    id: $0.id,
                    target: target,
                    authorPubkeyHex: $0.authorPubkey,
                    content: $0.content,
                    createdAt: Date(timeIntervalSince1970: TimeInterval($0.createdAt))
                )
            },
            acquisition: EpisodeCommentAcquisition(
                sourceCount: batch.evidence.sources.count,
                connectedSourceCount: batch.evidence.sources.filter { source in
                    if case .requesting = source.status { return true }
                    return false
                }.count,
                hasShortfall: !batch.evidence.shortfall.isEmpty,
                lastReconciledAt: batch.evidence.sources.compactMap(\.reconciledThrough)
                    .max()
                    .map { Date(timeIntervalSince1970: TimeInterval($0)) }
            )
        )
    }

    private func bridge(_ receipt: NMP.Receipt) -> EpisodeCommentReceipt {
        let (statuses, continuation) = AsyncStream.makeStream(of: EpisodeCommentWriteStatus.self)
        Task {
            for await status in receipt.status {
                continuation.yield(map(status))
            }
            continuation.finish()
        }
        return EpisodeCommentReceipt(id: receipt.id, statuses: statuses)
    }

    private func map(_ status: NMP.WriteStatus) -> EpisodeCommentWriteStatus {
        switch status {
        case .accepted: return .accepted
        case .awaitingCapability(let pubkey): return .awaitingCapability(pubkey: pubkey)
        case .signed(let eventID): return .signed(eventID: eventID)
        case .routed(let relays): return .routed(relays: relays)
        case .awaitingRelay(let relay): return .awaitingRelay(relay: relay)
        case .awaitingAuth(let relay): return .awaitingAuth(relay: relay)
        case .retryEligible(let relay, _, let eligibleAt):
            return .retryEligible(
                relay: relay,
                eligibleAt: Date(timeIntervalSince1970: TimeInterval(eligibleAt))
            )
        case .handoffAmbiguous(let relay, _, _): return .handoffAmbiguous(relay: relay)
        case .sent(let relay, _, _): return .sent(relay: relay)
        case .acked(let relay): return .acknowledged(relay: relay)
        case .rejected(let relay, let reason): return .rejected(relay: relay, reason: reason)
        case .gaveUp(let relay): return .gaveUp(relay: relay)
        case .persistenceBlocked(let relay), .routePersistenceBlocked(let relay):
            return .persistenceBlocked(relay: relay)
        case .outcomeUnknown(let relay): return .outcomeUnknown(relay: relay)
        case .replaceableConflict:
            return .failed(reason: "Comment conflicted with a retained write.")
        case .failed(let reason): return .failed(reason: reason)
        }
    }
}
#endif
