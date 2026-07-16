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
    case published(
        confirmedRelayCount: Int,
        unconfirmedRelayCount: Int,
        pendingRelayCount: Int
    )
    case rejected(String)
    case gaveUp(String)
    case persistenceBlocked(String)
    case failed(String)
    case deliveryUnknown(String)

    var label: String {
        switch self {
        case .queued: return "Queued"
        case .awaitingCapability: return "Waiting for signing capability"
        case .signed: return "Signed"
        case .delivering: return "Delivering"
        case .awaitingRelay: return "Waiting for a relay connection"
        case .awaitingRelayAuthorization: return "Waiting for relay authorization"
        case .retrying(let date):
            return "Retrying at \(date.formatted(date: .omitted, time: .shortened))"
        case .awaitingConfirmation: return "Sent; waiting for relay confirmation"
        case .published(let confirmed, let unconfirmed, let pending):
            var parts = [confirmed == 1 ? "1 relay confirmed" : "\(confirmed) relays confirmed"]
            if unconfirmed > 0 {
                parts.append(unconfirmed == 1 ? "1 unconfirmed" : "\(unconfirmed) unconfirmed")
            }
            if pending > 0 {
                parts.append(pending == 1 ? "1 still pending" : "\(pending) still pending")
            }
            return "Posted: " + parts.joined(separator: "; ")
        case .rejected(let message): return "Rejected: \(message)"
        case .gaveUp(let message): return "Delivery gave up: \(message)"
        case .persistenceBlocked(let message):
            return "Delivery could not be persisted: \(message)"
        case .failed(let message): return "Failed: \(message)"
        case .deliveryUnknown(let message): return "Outcome unknown: \(message)"
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
    private var reattachingReceiptIDs: Set<UInt64> = []
    private var receiptFacts: [UInt64: EpisodeCommentReceiptRollup] = [:]
    private var receiptRecords: [UInt64: PendingEpisodeCommentReceipt] = [:]
    private var draftAwaitingAcceptance: [UInt64: String] = [:]
    private var receiptsMissingRestartAnnotation: Set<UInt64> = []

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
            draftAwaitingAcceptance.isEmpty &&
            !draft.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
    }

    /// Cancellation withdraws only this read; durable receipt monitors continue.
    func observe(target: CommentTarget) async {
        isLoading = true
        loadError = nil
        activeAuthorPubkey = try? await repository.activeAuthorPubkey()
        await resumeReceipts(for: target)
        guard !Task.isCancelled else {
            isLoading = false
            return
        }

        do {
            let observation = try await repository.observe(target: target)
            defer { observation.cancel() }
            try await withTaskCancellationHandler {
                for try await snapshot in observation.updates {
                    guard !Task.isCancelled else { break }
                    comments = snapshot.comments.sorted {
                        if $0.createdAt != $1.createdAt { return $0.createdAt > $1.createdAt }
                        return $0.id < $1.id
                    }
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

    /// Persists a durable-write receipt and renders facts until canonical observation.
    func submit(target: CommentTarget) async {
        let submittedDraft = draft
        let content = submittedDraft.trimmingCharacters(in: .whitespacesAndNewlines)
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
            do {
                try receiptStore.save(record)
            } catch {
                submitError = error.localizedDescription
                receiptsMissingRestartAnnotation.insert(receipt.id)
            }
            upsertOutgoing(record, phase: .queued)
            draftAwaitingAcceptance[receipt.id] = submittedDraft
            beginMonitoring(receipt, record: record)
        } catch {
            submitError = error.localizedDescription
        }
    }

    private func resumeReceipts(for target: CommentTarget) async {
        let records: [PendingEpisodeCommentReceipt]
        do {
            records = try receiptStore.records(for: target)
        } catch {
            loadError = error.localizedDescription
            return
        }
        for record in records {
            await resumeReceipt(record)
        }
    }

    private func resumeReceipt(_ record: PendingEpisodeCommentReceipt) async {
        guard !activeReceiptIDs.contains(record.receiptID),
              reattachingReceiptIDs.insert(record.receiptID).inserted else { return }
        defer { reattachingReceiptIDs.remove(record.receiptID) }

        upsertOutgoing(record, phase: .queued)
        do {
            switch try await repository.reattachReceipt(id: record.receiptID) {
            case .attached(let receipt):
                beginMonitoring(receipt, record: record)
            case .notFound:
                try? receiptStore.remove(receiptID: record.receiptID)
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
        var facts = receiptFacts[receiptID] ?? EpisodeCommentReceiptRollup()
        facts.apply(status)
        receiptFacts[receiptID] = facts
        if case .signed(let eventID) = status,
           var record = receiptRecords[receiptID],
           record.eventID != eventID {
            record.eventID = eventID
            receiptRecords[receiptID] = record
            do {
                try receiptStore.save(record)
                receiptsMissingRestartAnnotation.remove(receiptID)
            } catch {
                submitError = error.localizedDescription
            }
        }
        releaseDraftIfAccepted(receiptID: receiptID, facts: facts)
        setPhase(facts.phase(streamEnded: false), receiptID: receiptID)
        reconcileCanonicalComments()
    }

    private func releaseDraftIfAccepted(receiptID: UInt64, facts: EpisodeCommentReceiptRollup) {
        guard facts.isDurablyAccepted,
              !receiptsMissingRestartAnnotation.contains(receiptID),
              let submittedDraft = draftAwaitingAcceptance.removeValue(forKey: receiptID) else { return }
        if draft == submittedDraft { draft = "" }
    }

    private func finishReceiptStream(receiptID: UInt64) {
        activeReceiptIDs.remove(receiptID)
        let facts = receiptFacts[receiptID] ?? EpisodeCommentReceiptRollup()
        let phase = facts.phase(streamEnded: true)
        setPhase(phase, receiptID: receiptID)
        if case .failed = phase, !facts.isDurablyAccepted {
            draftAwaitingAcceptance.removeValue(forKey: receiptID)
            receiptsMissingRestartAnnotation.remove(receiptID)
        }
        switch phase {
        case .published, .rejected, .gaveUp, .persistenceBlocked, .failed:
            try? receiptStore.remove(receiptID: receiptID)
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
            try? receiptStore.remove(receiptID: receiptID)
            receiptRecords.removeValue(forKey: receiptID)
            receiptFacts.removeValue(forKey: receiptID)
            outgoing.removeAll { $0.receiptID == receiptID }
        }
    }

    private func upsertOutgoing(_ record: PendingEpisodeCommentReceipt, phase: OutgoingEpisodeCommentPhase) {
        guard !outgoing.contains(where: { $0.receiptID == record.receiptID }) else { return }
        receiptRecords[record.receiptID] = record
        if receiptFacts[record.receiptID] == nil {
            receiptFacts[record.receiptID] = EpisodeCommentReceiptRollup(eventID: record.eventID)
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
