import Foundation

// MARK: - Kernel identity projection
//
// Narrow Swift mirror of the three NMP-core projection slots the iOS shell
// needs to drive the Identity surface:
//
//   projections.active_account   String?   — hex pubkey of the active account
//   projections.accounts         [Account] — every loaded identity (id, npub,
//                                            signer kind/label/is_remote, etc.)
//   projections.bunker_handshake DTO?      — NIP-46 broker handshake progress
//
// These keys live under `KernelSnapshot::projections` (see
// `crates/nmp-core/src/kernel/types.rs` and the inserts in
// `crates/nmp-core/src/kernel/update.rs`). The wire envelope reaches the iOS
// shell through `nmp_app_set_update_callback`; `KernelBridge.decode` runs both
// the existing `PodcastUpdate` decode (for `running`/`rev`/`schema_version`)
// AND this narrow projection decode (for identity). Two decoders, one parse
// pass per — keeps the wire types honest without forcing `PodcastUpdate` to
// model fields it doesn't otherwise consume.

/// One identity row from `projections.accounts` (per
/// `crates/nmp-core/src/kernel/identity_state.rs`'s `AccountSummary`).
///
/// Pre-classified fields (`signer_label`, `signer_is_remote`, `is_active`)
/// are the doctrine-blessed shape Swift binds — never string-match on
/// `signer_kind`.
struct KernelAccountSummary: Decodable, Equatable {
    /// Hex pubkey — the canonical identity id.
    let id: String
    /// Bech32 `npub1…` encoding of `id`.
    let npub: String
    /// kind:0 display name, when one has been fetched.
    let displayName: String?
    /// Stable wire token (`"local"` | `"nip46"` | …). Diagnostic only.
    let signerKind: String
    /// Pre-formatted English label (e.g. `"nsec"`, `"NIP-46"`).
    let signerLabel: String
    /// `true` when the signer's key material lives outside the kernel
    /// (NIP-46 bunker today). Drives the iOS "remote signer" branch.
    let signerIsRemote: Bool
    /// Pre-derived `status == "active"`.
    let isActive: Bool
    /// kind:0 profile picture URL, when one is cached.
    let pictureUrl: String?

    enum CodingKeys: String, CodingKey {
        case id
        case npub
        case displayName = "display_name"
        case signerKind = "signer_kind"
        case signerLabel = "signer_label"
        case signerIsRemote = "signer_is_remote"
        case isActive = "is_active"
        case pictureUrl = "picture_url"
    }
}

/// NIP-46 bunker handshake progress DTO from
/// `crates/nmp-core/src/actor/commands/identity.rs::BunkerHandshakeDto`.
///
/// `null` on the wire when no handshake is in flight; this struct decodes
/// only the populated case.
struct KernelBunkerHandshake: Decodable, Equatable {
    /// `"connecting"` | `"awaiting_pubkey"` | `"ready"` | `"failed"` |
    /// `"idle"` (the wire never carries `"idle"` from the actor today).
    let stage: String
    /// Human-readable status (relay url, error reason, …).
    let message: String?
    let isIdle: Bool
    let isInFlight: Bool
    let isFailed: Bool
    let isTerminalSuccess: Bool
    let canCancel: Bool
    /// Pre-formatted English label for the current stage — render verbatim.
    let stageLabel: String

    enum CodingKeys: String, CodingKey {
        case stage
        case message
        case isIdle = "is_idle"
        case isInFlight = "is_in_flight"
        case isFailed = "is_failed"
        case isTerminalSuccess = "is_terminal_success"
        case canCancel = "can_cancel"
        case stageLabel = "stage_label"
    }
}

/// Identity-slice of one kernel snapshot tick. All three fields may be empty
/// or `nil` when no identity is loaded and no handshake is in flight — that
/// is the steady-state for a fresh install.
struct KernelIdentityProjection: Equatable {
    /// Hex pubkey of the active account, or `nil` when no account is loaded.
    let activeAccount: String?
    /// All known identity rows, possibly empty.
    let accounts: [KernelAccountSummary]
    /// Bunker handshake progress, or `nil` when no handshake is in flight.
    let bunkerHandshake: KernelBunkerHandshake?

    static let empty = KernelIdentityProjection(
        activeAccount: nil, accounts: [], bunkerHandshake: nil)

    /// The active row (if any), looked up by pubkey.
    var activeAccountRow: KernelAccountSummary? {
        guard let hex = activeAccount else { return nil }
        return accounts.first { $0.id == hex }
    }

    /// `true` when the active account is NIP-46 (or any future remote signer).
    /// Reads `signer_is_remote` directly per NMP doctrine — never string-
    /// matches on `signer_kind`.
    var isRemoteSigner: Bool {
        activeAccountRow?.signerIsRemote ?? false
    }
}

// MARK: - Decode helper

extension KernelIdentityProjection {
    /// Decode the identity projection slice from one already-parsed kernel
    /// snapshot wire envelope (`{"t":"snapshot","v":<KernelSnapshot>}`).
    ///
    /// The pruning logic mirrors the doctrine: when `projections.accounts`
    /// is missing, we still surface whatever subset arrived (forward-compat).
    /// Decode failures degrade to `.empty` so a malformed projection never
    /// nukes the surrounding snapshot.
    static func decode(envelopePayload data: Data) -> KernelIdentityProjection {
        guard let raw = try? JSONSerialization.jsonObject(with: data),
              let outer = raw as? [String: Any],
              let value = outer["v"] as? [String: Any],
              let projections = value["projections"] as? [String: Any]
        else { return .empty }

        let active = projections["active_account"] as? String

        let accounts: [KernelAccountSummary] = {
            guard let arr = projections["accounts"] as? [[String: Any]],
                  let json = try? JSONSerialization.data(withJSONObject: arr)
            else { return [] }
            return (try? JSONDecoder().decode([KernelAccountSummary].self, from: json)) ?? []
        }()

        let handshake: KernelBunkerHandshake? = {
            guard let obj = projections["bunker_handshake"] as? [String: Any],
                  let json = try? JSONSerialization.data(withJSONObject: obj)
            else { return nil }
            return try? JSONDecoder().decode(KernelBunkerHandshake.self, from: json)
        }()

        return KernelIdentityProjection(
            activeAccount: active, accounts: accounts, bunkerHandshake: handshake)
    }
}
