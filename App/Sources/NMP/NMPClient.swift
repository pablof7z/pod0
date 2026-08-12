import Foundation
import NMP
import Pod0Core

/// Pod0's one NMP adoption point. Identity, secrets, signing, routing,
/// transport, queries, and receipts remain owned by the upstream SDK.
actor NMPClient {
    private var engine: NMPEngine?
    private var activeReceiptIDs: Set<UInt64> = []

    private func requireEngine() throws -> NMPEngine {
        if let engine { return engine }
        guard let support = FileManager.default.urls(
            for: .applicationSupportDirectory,
            in: .userDomainMask
        ).first else {
            throw NMPClientError.unavailable
        }
        try FileManager.default.createDirectory(
            at: support,
            withIntermediateDirectories: true
        )
        let created = try NMPEngine(
            config: NMPConfig(
                storePath: support.appendingPathComponent("nmp.redb").path,
                appRelays: ["wss://relay.primal.net", "wss://relay.damus.io"],
                fallbackRelays: ["wss://relay.primal.net", "wss://relay.damus.io"]
            ),
            localAccountStore: NMPKeychainAccountStore(
                service: "io.f7z.podcast.nmp",
                account: "local-account"
            )
        )
        engine = created
        return created
    }

    @discardableResult
    func ensureAccount() async throws -> String {
        let engine = try requireEngine()
        if let active = try engine.activeAccount() { return active }
        let account = try await engine.generateAccount()
        try engine.setActiveAccount(account.publicKey)
        return account.publicKey
    }

    func signOut() throws {
        let engine = try requireEngine()
        _ = try engine.detachPersistedAccount()
        try engine.setActiveAccount(nil)
    }

    func resume(from facade: Pod0Facade) {
        let links = facade.nmpPublicationReceiptLinks()
        for link in links {
            Task { await reattach(link, to: facade) }
        }
        publishPending(from: facade)
    }

    func publishPending(from facade: Pod0Facade) {
        for draft in facade.nextNmpPublications(maximumCount: 8) {
            Task { await publish(draft, to: facade) }
        }
    }

    private func publish(_ draft: Pod0PublicationDraft, to facade: Pod0Facade) async {
        do {
            let engine = try requireEngine()
            let receipt = try await engine.publish(WriteIntent(
                payload: .event(
                    kind: draft.kind,
                    tags: draft.tags,
                    content: draft.content,
                    createdAt: draft.createdAtSeconds
                ),
                routing: .auto,
                identity: .explicit(pubkey: draft.expectedAuthorHex),
                correlation: draft.correlationToken
            ))
            facade.recordNmpPublicationReceipt(
                publicationId: draft.publicationId,
                receiptId: receipt.id
            )
            facade.recordNmpPublicationObservation(
                publicationId: draft.publicationId,
                observation: .nmpAccepted
            )
            await consume(receipt, publicationID: draft.publicationId, in: facade)
        } catch {
            facade.recordNmpPublicationObservation(
                publicationId: draft.publicationId,
                observation: .nmpFailure("NMP publication failed")
            )
        }
    }

    private func reattach(_ link: NmpPublicationReceiptLink, to facade: Pod0Facade) async {
        guard !activeReceiptIDs.contains(link.receiptId) else { return }
        do {
            switch try requireEngine().reattachReceipt(id: link.receiptId) {
            case .attached(let receipt):
                await consume(receipt, publicationID: link.publicationId, in: facade)
            case .notFound:
                facade.recordNmpPublicationObservation(
                    publicationId: link.publicationId,
                    observation: .nmpReattachmentNotFound
                )
            case .retainedButUnreadable:
                facade.recordNmpPublicationObservation(
                    publicationId: link.publicationId,
                    observation: .nmpReattachmentUnreadable
                )
            }
        } catch {
            facade.recordNmpPublicationObservation(
                publicationId: link.publicationId,
                observation: .nmpFailure("NMP receipt reattachment failed")
            )
        }
    }

    private func consume(
        _ receipt: Receipt,
        publicationID: PublicationId,
        in facade: Pod0Facade
    ) async {
        guard activeReceiptIDs.insert(receipt.id).inserted else { return }
        defer { activeReceiptIDs.remove(receipt.id) }
        do {
            for try await fact in receipt.status {
                for observation in fact.pod0Observations {
                    facade.recordNmpPublicationObservation(
                        publicationId: publicationID,
                        observation: observation
                    )
                }
            }
        } catch {
            facade.recordNmpPublicationObservation(
                publicationId: publicationID,
                observation: .nmpFailure("NMP receipt stream interrupted")
            )
        }
    }
}

private enum NMPClientError: Error {
    case unavailable
}
