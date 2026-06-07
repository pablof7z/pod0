import Foundation

// MARK: - Kernel identity projection
//
// Swift mirror of the identity fields surfaced via `PodcastUpdate.activeAccount`
// (inside `projections["podcast.snapshot"]`). Built by
// `KernelIdentityProjection.from(podcastUpdate:)` in KernelBridge — NOT by
// parsing the top-level `projections` dictionary, which has no identity keys.
//
// Current wire surface:
//   PodcastUpdate.active_account  AccountSummary?  — npub + pubkey_hex + mode + display_name
//
// Future slots (not yet wired in Rust; degrade to nil/empty gracefully):
//   accounts         [Account]  — all loaded accounts for multi-identity
//   bunker_handshake DTO?       — NIP-46 handshake progress
//   resolved_profiles map       — pubkey→profile cache

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

/// One row from `projections.resolved_profiles` — the kernel's pre-merged
/// pubkey → profile map (NMP v0.2.0+). Populated by NMP for every pubkey the
/// kernel already knows about, so these profiles resolve without a Swift-side
/// relay round-trip.
///
/// `display` maps to `display_name` on the wire — the kernel's merged best
/// display name (NIP-05 > display_name > name); `pictureUrl` is the kind:0 picture.
/// Both are optional — the kernel may have only one or neither.
struct ResolvedProfile: Decodable, Equatable {
    let display: String?
    let pictureUrl: String?

    enum CodingKeys: String, CodingKey {
        case display = "display_name"
        case pictureUrl = "picture_url"
    }
}

/// Identity-slice of one kernel snapshot tick. All fields may be empty
/// or `nil` when no identity is loaded and no handshake is in flight — that
/// is the steady-state for a fresh install.
struct KernelIdentityProjection: Equatable {
    /// Hex pubkey of the active account, or `nil` when no account is loaded.
    /// Sourced from `PodcastUpdate.active_account.pubkey_hex`.
    let activeAccount: String?
    /// Bech32 npub of the active account (`npub1…`) for display surfaces.
    /// Sourced from `PodcastUpdate.active_account.npub`.
    let activeNpub: String?
    /// All known identity rows, possibly empty.
    let accounts: [KernelAccountSummary]
    /// Bunker handshake progress, or `nil` when no handshake is in flight.
    let bunkerHandshake: KernelBunkerHandshake?
    /// Kernel-resolved profiles keyed by hex pubkey (`projections.resolved_profiles`,
    /// NMP v0.2.0+). Additive: pre-populated by the kernel and merged into the
    /// app's `nostrProfileCache` so agent-conversation pubkeys render without a
    /// dedicated relay subscription. Empty on kernels that predate the slot.
    let resolvedProfiles: [String: ResolvedProfile]

    static let empty = KernelIdentityProjection(
        activeAccount: nil,
        activeNpub: nil,
        accounts: [],
        bunkerHandshake: nil,
        resolvedProfiles: [:])

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

// MARK: - Factory from decoded PodcastUpdate

extension KernelIdentityProjection {
    /// Build the identity projection from an already-decoded `PodcastUpdate`.
    ///
    /// `active_account` is a field on `PodcastUpdate` (inside the
    /// `projections["podcast.snapshot"]` slice), NOT at the top-level
    /// `projections` dictionary. Constructing from the typed struct avoids
    /// the raw-JSON path that previously read the wrong level and returned
    /// `.empty` for every snapshot tick, causing "No identity" to persist
    /// across restarts even when the kernel had a saved account.
    ///
    /// `accounts`, `bunkerHandshake`, and `resolvedProfiles` remain empty
    /// until the Rust backend adds dedicated top-level projection slots for
    /// them. Their absence degrades gracefully: NIP-46 handshake UI is
    /// hidden when `bunkerHandshake == nil`, and resolved-profile lookups
    /// fall back to relay fetches.
    static func from(podcastUpdate update: PodcastUpdate) -> KernelIdentityProjection {
        KernelIdentityProjection(
            activeAccount: update.activeAccount?.pubkeyHex,
            activeNpub: update.activeAccount?.npub,
            accounts: [],
            bunkerHandshake: nil,
            resolvedProfiles: [:])
    }
}
