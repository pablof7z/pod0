import Foundation

/// Scoped acquisition facts for one comments observation. These are facts,
/// not a claim that the relay network is globally complete.
struct EpisodeCommentAcquisition: Equatable, Sendable {
    var sourceCount: Int
    var connectedSourceCount: Int
    var hasShortfall: Bool
    var lastReconciledAt: Date?

    static let starting = EpisodeCommentAcquisition(
        sourceCount: 0,
        connectedSourceCount: 0,
        hasShortfall: false,
        lastReconciledAt: nil
    )
}

struct EpisodeCommentSnapshot: Equatable, Sendable {
    var comments: [EpisodeComment]
    var acquisition: EpisodeCommentAcquisition
}

/// A live NMP observation. Cancellation withdraws read demand only; it never
/// cancels or retries a write obligation.
final class EpisodeCommentObservation: @unchecked Sendable {
    let updates: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>

    private let lock = NSLock()
    private var didCancel = false
    private let cancelAction: @Sendable () -> Void

    init(
        updates: AsyncThrowingStream<EpisodeCommentSnapshot, any Error>,
        cancel: @escaping @Sendable () -> Void
    ) {
        self.updates = updates
        self.cancelAction = cancel
    }

    func cancel() {
        let shouldCancel = lock.withLock {
            guard !didCancel else { return false }
            didCancel = true
            return true
        }
        if shouldCancel { cancelAction() }
    }

    deinit { cancel() }
}

enum EpisodeCommentWriteStatus: Equatable, Sendable {
    case accepted
    case awaitingCapability(pubkey: String)
    case signed(eventID: String)
    case routed(relays: [String])
    case awaitingRelay(relay: String)
    case awaitingAuth(relay: String)
    case retryEligible(relay: String, eligibleAt: Date)
    case handoffAmbiguous(relay: String)
    case sent(relay: String)
    case acknowledged(relay: String)
    case rejected(relay: String, reason: String)
    case gaveUp(relay: String)
    case persistenceBlocked(relay: String)
    case outcomeUnknown(relay: String)
    case failed(reason: String)
}

struct EpisodeCommentReceipt: Sendable {
    let id: UInt64
    let statuses: AsyncStream<EpisodeCommentWriteStatus>
}

enum EpisodeCommentReceiptReattachment: Sendable {
    case attached(EpisodeCommentReceipt)
    case notFound
    case retainedButUnreadable
}

/// Pod0 consumes semantic comments and receipt facts only. The live adapter
/// delegates event composition, verification, routing, signing, persistence,
/// and relay ownership to NMP.
protocol EpisodeCommentsRepository: Sendable {
    func activeAuthorPubkey() async throws -> String?
    func observe(target: CommentTarget) async throws -> EpisodeCommentObservation
    func publish(content: String, target: CommentTarget) async throws -> EpisodeCommentReceipt
    func reattachReceipt(id: UInt64) async throws -> EpisodeCommentReceiptReattachment
}

struct UnavailableEpisodeCommentsRepository: EpisodeCommentsRepository {
    func activeAuthorPubkey() async throws -> String? { nil }

    func observe(target: CommentTarget) async throws -> EpisodeCommentObservation {
        throw EpisodeCommentsRepositoryError.unavailable
    }

    func publish(content: String, target: CommentTarget) async throws -> EpisodeCommentReceipt {
        throw EpisodeCommentsRepositoryError.unavailable
    }

    func reattachReceipt(id: UInt64) async throws -> EpisodeCommentReceiptReattachment {
        throw EpisodeCommentsRepositoryError.unavailable
    }
}

enum EpisodeCommentsRepositoryError: LocalizedError {
    case unavailable

    var errorDescription: String? {
        "Comments are temporarily unavailable."
    }
}
