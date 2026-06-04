import Foundation

// MARK: - Event types

/// Unsigned event draft as accepted by `NostrSigner.sign(_:)`.
/// Mirrors the NIP-01 event JSON minus `id` / `pubkey` / `sig` (the signer fills those).
struct NostrEventDraft: Sendable, Equatable {
    var kind: Int
    var content: String
    var tags: [[String]]
    /// `created_at` UNIX seconds. Defaults to "now" at construction.
    var createdAt: Int

    init(kind: Int, content: String, tags: [[String]] = [], createdAt: Int = Int(Date().timeIntervalSince1970)) {
        self.kind = kind
        self.content = content
        self.tags = tags
        self.createdAt = createdAt
    }
}

// Note: `SignedNostrEvent` is the kernel-projected data carrier defined in
// `Features/Feedback/SignedNostrEvent.swift`. It is NOT redefined here — Swift
// never constructs or signs events; the kernel fills `id` / `pubkey` / `sig`.

// MARK: - Signer protocol

/// Anything that can produce a Nostr signature. After the kernel-owned signing
/// migration (NMP v0.2.4 / D13) the only production implementation is
/// `KernelSigner`; the Swift-held private-key signers (`LocalKeySigner` /
/// `RemoteSigner`) were deleted along with the Swift Nostr crypto.
protocol NostrSigner: Sendable {
    /// The user-facing pubkey this signer publishes events under (32-byte hex x-only).
    func publicKey() async throws -> String
    /// Compute the canonical event id, sign it, and return the wire-ready event.
    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent
}

// MARK: - Kernel signer (D13 — the kernel holds the key, Swift never does)

/// `NostrSigner` that signs through the NMP kernel via
/// `nmp_app_sign_event_for_return`. NO private key, schnorr code, or event-id
/// hashing runs in Swift — the kernel owns the key on the actor thread and
/// returns the fully-signed wire event. This is the compliant replacement for
/// the retired `LocalKeySigner` (the D13 violation).
///
/// `accountPubkeyHex` selects which kernel account signs (empty string → the
/// active account). The signer is stateless beyond the `KernelModel` reference;
/// construct it per upload from `appStore.kernel`.
struct KernelSigner: NostrSigner {
    /// The kernel bridge that performs the sign-for-return round-trip.
    let kernel: KernelModel
    /// Hex pubkey of the signing account. Empty selects the kernel's active
    /// account; an explicit value targets a specific registered signer (e.g.
    /// an agent-owned podcast key).
    let accountPubkeyHex: String

    init(kernel: KernelModel, accountPubkeyHex: String = "") {
        self.kernel = kernel
        self.accountPubkeyHex = accountPubkeyHex
    }

    func publicKey() async throws -> String {
        if !accountPubkeyHex.isEmpty { return accountPubkeyHex }
        guard let active = await kernel.kernelIdentity.activeAccount, !active.isEmpty else {
            throw NostrSignerError.missingPublicKey
        }
        return active
    }

    func sign(_ draft: NostrEventDraft) async throws -> SignedNostrEvent {
        // The kernel re-stamps `created_at` (D7) and fills `pubkey`/`id`/`sig`;
        // the draft carries only kind/content/tags. `created_at` is advisory.
        let unsigned: [String: Any] = [
            "kind": draft.kind,
            "content": draft.content,
            "tags": draft.tags,
            "created_at": draft.createdAt,
        ]
        guard
            let data = try? JSONSerialization.data(withJSONObject: unsigned),
            let unsignedJSON = String(data: data, encoding: .utf8)
        else {
            throw NostrSignerError.invalidEventForSigning
        }
        let signedJSON = try await kernel.signEventForReturn(
            accountPubkeyHex: accountPubkeyHex, unsignedJSON: unsignedJSON)
        guard let eventData = signedJSON.data(using: .utf8) else {
            throw NostrSignerError.invalidEventForSigning
        }
        do {
            return try JSONDecoder().decode(SignedNostrEvent.self, from: eventData)
        } catch {
            throw NostrSignerError.remoteRejected(
                "kernel returned an undecodable signed event: \(error.localizedDescription)")
        }
    }
}

enum NostrSignerError: LocalizedError {
    case invalidEventForSigning
    case remoteRejected(String)
    case timedOut
    case notConnected
    case missingPublicKey

    var errorDescription: String? {
        switch self {
        case .invalidEventForSigning: "Could not sign — event payload is invalid."
        case .remoteRejected(let m): "Remote signer rejected the request: \(m)"
        case .timedOut: "Remote signer did not respond in time."
        case .notConnected: "Remote signer is not connected."
        case .missingPublicKey: "Remote signer has not advertised a public key yet."
        }
    }
}
