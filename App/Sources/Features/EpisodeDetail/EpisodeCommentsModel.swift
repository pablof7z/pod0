import Foundation
import Observation

enum OutgoingEpisodeCommentPhase: Equatable, Sendable {
    case queued
    case awaitingCapability
    case signed
    case delivering
    case awaitingRelay
    case awaitingRelayAuthorization
    case retrying(eligibleAt: Date)
    case awaitingConfirmation
    case published(relayCount: Int)
    case rejected(String)
    case gaveUp(String)
    case persistenceBlocked(String)
    case failed(String)
    case deliveryUnknown(String)

    var label: String {
        switch self {
        case .queued: "Queued"
        case .awaitingCapability: "Waiting for signing capability"
        case .signed: "Signed"
        case .delivering: "Delivering"
        case .awaitingRelay: "Waiting for a relay connection"
        case .awaitingRelayAuthorization: "Waiting for relay authorization"
        case .retrying(let date): "Retrying at \(date.formatted(date: .omitted, time: .shortened))"
        case .awaitingConfirmation: "Sent; waiting for relay confirmation"
        case .published(let count): count == 1 ? "Published to 1 relay" : "Published to \(count) relays"
        case .rejected(let message): "Rejected: \(message)"
        case .gaveUp(let message): "Delivery gave up: \(message)"
        case .persistenceBlocked(let message): "Delivery could not be persisted: \(message)"
        case .failed(let message): "Failed: \(message)"
        case .deliveryUnknown(let message): "Outcome unknown: \(message)"
        }
    }
}

struct OutgoingEpisodeComment: Identifiable, Equatable, Sendable {
    var id: UInt64 { receiptID }

    let receiptID: UInt64
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
    private var receiptRecords: [UInt64: PendingEpisodeCommentReceipt] = [:]

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
                eventID: nil,
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
        if case .signed(let eventID) = status,
           var record = receiptRecords[receiptID],
           record.eventID != eventID {
            record.eventID = eventID
            receiptRecords[receiptID] = record
            receiptStore.save(record)
        }
        setPhase(facts.phase(streamEnded: false), receiptID: receiptID)
        reconcileCanonicalComments()
    }

    private func finishReceiptStream(receiptID: UInt64) {
        activeReceiptIDs.remove(receiptID)
        let facts = receiptFacts[receiptID] ?? ReceiptFacts()
        let phase = facts.phase(streamEnded: true)
        setPhase(phase, receiptID: receiptID)
        switch phase {
        case .published, .rejected, .gaveUp, .persistenceBlocked, .failed:
            receiptStore.remove(receiptID: receiptID)
            receiptRecords.removeValue(forKey: receiptID)
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
            receiptRecords.removeValue(forKey: receiptID)
            receiptFacts.removeValue(forKey: receiptID)
            outgoing.removeAll { $0.receiptID == receiptID }
        }
    }

    private func upsertOutgoing(
        _ record: PendingEpisodeCommentReceipt,
        phase: OutgoingEpisodeCommentPhase
    ) {
        guard !outgoing.contains(where: { $0.receiptID == record.receiptID }) else { return }
        receiptRecords[record.receiptID] = record
        if receiptFacts[record.receiptID] == nil {
            receiptFacts[record.receiptID] = ReceiptFacts(eventID: record.eventID)
        }
        outgoing.insert(
            OutgoingEpisodeComment(
                receiptID: record.receiptID,
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
    var latest: EpisodeCommentWriteStatus = .accepted

    mutating func apply(_ status: EpisodeCommentWriteStatus) {
        latest = status
        switch status {
        case .signed(let id): eventID = id
        case .routed(let relays): routedRelays.formUnion(relays)
        case .acknowledged(let relay): acknowledgedRelays.insert(relay)
        default: break
        }
    }

    func phase(streamEnded: Bool) -> OutgoingEpisodeCommentPhase {
        if !acknowledgedRelays.isEmpty {
            return .published(relayCount: acknowledgedRelays.count)
        }
        switch latest {
        case .accepted: return .queued
        case .awaitingCapability: return .awaitingCapability
        case .signed: return .signed
        case .awaitingRelay: return .awaitingRelay
        case .sent, .handoffAmbiguous: return .awaitingConfirmation
        case .awaitingAuth: return .awaitingRelayAuthorization
        case .retryEligible(_, let eligibleAt): return .retrying(eligibleAt: eligibleAt)
        case .rejected(let relay, let reason): return .rejected("\(relay): \(reason)")
        case .gaveUp(let relay): return .gaveUp(relay)
        case .persistenceBlocked(let relay): return .persistenceBlocked(relay)
        case .outcomeUnknown(let relay): return .deliveryUnknown(relay)
        case .failed(let reason): return .failed(reason)
        default:
            return streamEnded
                ? .deliveryUnknown("Delivery ended without relay confirmation.")
                : .delivering
        }
    }
}
