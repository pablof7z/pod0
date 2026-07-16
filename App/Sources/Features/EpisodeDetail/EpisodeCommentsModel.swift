import Foundation
import Observation

enum OutgoingEpisodeCommentPhase: Equatable, Sendable {
    case queued
    case awaitingApproval
    case signed
    case delivering
    case awaitingRelayAuthorization
    case awaitingConfirmation
    case published(relayCount: Int)
    case failed(String)
    case deliveryUnknown(String)

    var label: String {
        switch self {
        case .queued: "Queued"
        case .awaitingApproval: "Waiting for signing approval"
        case .signed: "Signed"
        case .delivering: "Delivering"
        case .awaitingRelayAuthorization: "Waiting for relay authorization"
        case .awaitingConfirmation: "Sent; waiting for relay confirmation"
        case .published(let count): count == 1 ? "Published to 1 relay" : "Published to \(count) relays"
        case .failed(let message), .deliveryUnknown(let message): message
        }
    }
}

struct OutgoingEpisodeComment: Identifiable, Equatable, Sendable {
    var id: UInt64 { receiptID }

    let receiptID: UInt64
    let content: String
    let submittedAt: Date
    var phase: OutgoingEpisodeCommentPhase
}

@MainActor
@Observable
final class EpisodeCommentsModel {
    var draft = ""
    private(set) var comments: [EpisodeComment] = []
    private(set) var acquisition = EpisodeCommentAcquisition.starting
    private(set) var outgoing: [OutgoingEpisodeComment] = []
    private(set) var activeAuthorPubkey: String?
    private(set) var isLoading = false
    private(set) var isSubmitting = false
    private(set) var loadError: String?
    private(set) var submitError: String?

    private let repository: any EpisodeCommentsRepository
    private let receiptStore: any EpisodeCommentReceiptStore
    private var activeReceiptIDs: Set<UInt64> = []
    private var receiptFacts: [UInt64: ReceiptFacts] = [:]

    init(
        repository: any EpisodeCommentsRepository,
        receiptStore: any EpisodeCommentReceiptStore = UserDefaultsEpisodeCommentReceiptStore()
    ) {
        self.repository = repository
        self.receiptStore = receiptStore
    }

    var canSubmit: Bool {
        activeAuthorPubkey != nil &&
            !isSubmitting &&
            !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Runs one read session. SwiftUI task cancellation withdraws only this
    /// observation; durable receipt monitors continue independently.
    func observe(target: CommentTarget) async {
        isLoading = true
        loadError = nil
        activeAuthorPubkey = try? await repository.activeAuthorPubkey()
        await resumeReceipts(for: target)

        do {
            let observation = try await repository.observe(target: target)
            defer { observation.cancel() }
            try await withTaskCancellationHandler {
                for try await snapshot in observation.updates {
                    guard !Task.isCancelled else { break }
                    comments = snapshot.comments.sorted { $0.createdAt > $1.createdAt }
                    acquisition = snapshot.acquisition
                    isLoading = false
                    reconcileCanonicalComments()
                }
            } onCancel: {
                observation.cancel()
            }
            if !Task.isCancelled, isLoading {
                isLoading = false
            }
        } catch is CancellationError {
            isLoading = false
        } catch {
            isLoading = false
            loadError = error.localizedDescription
        }
    }

    /// Enqueues a durable write, persists its NMP receipt id immediately, and
    /// renders only receipt facts until the canonical observation sees it.
    func submit(target: CommentTarget) async {
        let content = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        guard canSubmit, !content.isEmpty else { return }
        isSubmitting = true
        submitError = nil
        defer { isSubmitting = false }

        do {
            let receipt = try await repository.publish(content: content, target: target)
            let record = PendingEpisodeCommentReceipt(
                receiptID: receipt.id,
                target: target,
                content: content,
                submittedAt: Date()
            )
            receiptStore.save(record)
            upsertOutgoing(record, phase: .queued)
            draft = ""
            beginMonitoring(receipt, record: record)
        } catch {
            submitError = error.localizedDescription
        }
    }

    private func resumeReceipts(for target: CommentTarget) async {
        for record in receiptStore.records(for: target) where !activeReceiptIDs.contains(record.receiptID) {
            upsertOutgoing(record, phase: .queued)
            do {
                switch try await repository.reattachReceipt(id: record.receiptID) {
                case .attached(let receipt):
                    beginMonitoring(receipt, record: record)
                case .notFound:
                    receiptStore.remove(receiptID: record.receiptID)
                    setPhase(
                        .deliveryUnknown("Delivery record is no longer available."),
                        receiptID: record.receiptID
                    )
                case .retainedButUnreadable:
                    setPhase(
                        .deliveryUnknown("Delivery record exists but could not be read."),
                        receiptID: record.receiptID
                    )
                }
            } catch {
                setPhase(.deliveryUnknown(error.localizedDescription), receiptID: record.receiptID)
            }
        }
    }

    private func beginMonitoring(_ receipt: EpisodeCommentReceipt, record _: PendingEpisodeCommentReceipt) {
        guard activeReceiptIDs.insert(receipt.id).inserted else { return }
        Task { [weak self] in
            guard let self else { return }
            for await status in receipt.statuses {
                self.apply(status, receiptID: receipt.id)
            }
            self.finishReceiptStream(receiptID: receipt.id)
        }
    }

    private func apply(_ status: EpisodeCommentWriteStatus, receiptID: UInt64) {
        var facts = receiptFacts[receiptID] ?? ReceiptFacts()
        facts.apply(status)
        receiptFacts[receiptID] = facts
        setPhase(facts.phase(streamEnded: false), receiptID: receiptID)
        reconcileCanonicalComments()
    }

    private func finishReceiptStream(receiptID: UInt64) {
        activeReceiptIDs.remove(receiptID)
        let facts = receiptFacts[receiptID] ?? ReceiptFacts()
        let phase = facts.phase(streamEnded: true)
        setPhase(phase, receiptID: receiptID)
        switch phase {
        case .published, .failed:
            receiptStore.remove(receiptID: receiptID)
        default:
            break
        }
    }

    private func reconcileCanonicalComments() {
        let canonicalIDs = Set(comments.map(\.id))
        let observedReceipts = receiptFacts.compactMap { receiptID, facts in
            facts.eventID.map { canonicalIDs.contains($0) ? receiptID : nil } ?? nil
        }
        for receiptID in observedReceipts {
            receiptStore.remove(receiptID: receiptID)
            outgoing.removeAll { $0.receiptID == receiptID }
        }
    }

    private func upsertOutgoing(
        _ record: PendingEpisodeCommentReceipt,
        phase: OutgoingEpisodeCommentPhase
    ) {
        guard !outgoing.contains(where: { $0.receiptID == record.receiptID }) else { return }
        outgoing.insert(
            OutgoingEpisodeComment(
                receiptID: record.receiptID,
                content: record.content,
                submittedAt: record.submittedAt,
                phase: phase
            ),
            at: 0
        )
    }

    private func setPhase(_ phase: OutgoingEpisodeCommentPhase, receiptID: UInt64) {
        guard let index = outgoing.firstIndex(where: { $0.receiptID == receiptID }) else { return }
        outgoing[index].phase = phase
    }
}

private struct ReceiptFacts {
    var eventID: String?
    var routedRelays: Set<String> = []
    var acknowledgedRelays: Set<String> = []
    var terminalFailures: [String] = []
    var latest: EpisodeCommentWriteStatus = .accepted

    mutating func apply(_ status: EpisodeCommentWriteStatus) {
        latest = status
        switch status {
        case .signed(let id): eventID = id
        case .routed(let relays): routedRelays.formUnion(relays)
        case .acknowledged(let relay): acknowledgedRelays.insert(relay)
        case .rejected(let relay, let reason): terminalFailures.append("\(relay): \(reason)")
        case .gaveUp(let relay): terminalFailures.append("\(relay) gave up")
        case .persistenceBlocked(let relay): terminalFailures.append("\(relay) persistence blocked")
        case .outcomeUnknown(let relay): terminalFailures.append("\(relay) outcome unknown")
        case .failed(let reason): terminalFailures.append(reason)
        default: break
        }
    }

    func phase(streamEnded: Bool) -> OutgoingEpisodeCommentPhase {
        if !acknowledgedRelays.isEmpty {
            return .published(relayCount: acknowledgedRelays.count)
        }
        if streamEnded {
            if let failure = terminalFailures.first { return .failed(failure) }
            return .deliveryUnknown("Delivery ended without relay confirmation.")
        }
        switch latest {
        case .accepted: return .queued
        case .awaitingCapability: return .awaitingApproval
        case .signed: return .signed
        case .sent, .handoffAmbiguous: return .awaitingConfirmation
        case .awaitingAuth: return .awaitingRelayAuthorization
        case .failed(let reason): return .failed(reason)
        default: return .delivering
        }
    }
}
