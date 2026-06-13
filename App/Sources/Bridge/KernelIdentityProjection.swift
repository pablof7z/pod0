import Foundation

// MARK: - Kernel identity projection
//
// Swift mirror of the identity fields surfaced via the kernel push frame.
// Built by `KernelIdentityProjection.from(domainFrames:)` for the push path
// and `from(podcastUpdate:)` for the pull/cold-start path.
//
// Current wire surface:
//   podcast.identity.active_account    AccountSummary?       — npub + pubkey_hex + mode
//   projections["resolved_profiles"]   [String:ResolvedProfile] — pubkey→kind:0 cache
//                                       (top-level projections key, NOT a podcast.* sidecar)
//
// Future slots (not yet wired in Rust; degrade to nil/empty gracefully):
//   accounts         [Account]  — all loaded accounts for multi-identity
//   bunker_handshake DTO?       — NIP-46 handshake progress

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
/// Wire keys are snake_case from Rust (`display_name`, `picture_url`).
/// The bridge decoder uses `.convertFromSnakeCase`, so these properties carry
/// camelCase names (`displayName`, `pictureUrl`) with NO explicit CodingKeys.
/// Explicit snake_case CodingKeys would override the strategy and cause
/// `keyNotFound` for every key, silently dropping all profiles (#371 contract).
///
/// Android counterpart uses `@SerialName("display_name")` because
/// kotlinx-serialization does NOT auto-convert snake_case without explicit config.
struct ResolvedProfile: Decodable, Equatable {
    /// Kernel's merged best display name (`display_name` on the wire).
    /// Decoded via `.convertFromSnakeCase`: `display_name` → `displayName`.
    let displayName: String?
    /// kind:0 picture URL (`picture_url` on the wire).
    /// Decoded via `.convertFromSnakeCase`: `picture_url` → `pictureUrl`.
    let pictureUrl: String?
    // NO CodingKeys — .convertFromSnakeCase handles snake_case → camelCase.
    // Adding explicit snake_case CodingKeys here would silently break decoding.
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

// MARK: - Factory from decoded PodcastUpdate (legacy — kept for pull path)

extension KernelIdentityProjection {
    /// Build the identity projection from an already-decoded `PodcastUpdate`.
    ///
    /// Used by the pull path (`podcastSnapshot()`) and cold-start hydration.
    /// The push path uses `from(domainFrames:)` instead.
    ///
    /// `resolvedProfiles` is `[:]` here because the pull path (`PodcastUpdate`)
    /// does not carry the top-level `projections["resolved_profiles"]` key — that
    /// key lives in the push-frame envelope and is decoded by `PodcastDomainFrames.decode`.
    /// The pull path is a cold-start / compatibility path; resolved profiles
    /// accumulate in `kernelIdentity` via the push path over subsequent ticks.
    static func from(podcastUpdate update: PodcastUpdate) -> KernelIdentityProjection {
        KernelIdentityProjection(
            activeAccount: update.activeAccount?.pubkeyHex,
            activeNpub: update.activeAccount?.npub,
            accounts: [],
            bunkerHandshake: nil,
            resolvedProfiles: [:])
    }
}

// MARK: - Factory from per-domain push frames

extension KernelIdentityProjection {
    /// Build the identity projection from per-domain push-frame sidecars.
    ///
    /// The `podcast.identity` sidecar carries `active_account`; it is only
    /// present when the identity domain changed since the last emit. When it
    /// is absent (no identity change this tick), the caller is responsible for
    /// preserving the previous `kernelIdentity` value — this factory is called
    /// only when the identity domain IS present in the frame.
    ///
    /// `accounts` and `bunkerHandshake` remain empty until the kernel adds
    /// dedicated projection slots — their absence degrades gracefully (NIP-46
    /// UI hidden). `resolvedProfiles` is sourced from the top-level
    /// `projections["resolved_profiles"]` key decoded into `frames.resolvedProfiles`.
    ///
    /// This factory is called when the identity domain IS present in the frame.
    /// When it is absent, `KernelModel.apply` merges resolved profiles directly
    /// into the cached `kernelIdentity` — that path does not call this factory.
    static func from(domainFrames frames: PodcastDomainFrames) -> KernelIdentityProjection {
        // Identity domain is the authoritative source for account fields.
        if let identityFrame = frames.identity {
            return KernelIdentityProjection(
                activeAccount: identityFrame.activeAccount?.pubkeyHex,
                activeNpub: identityFrame.activeAccount?.npub,
                accounts: [],
                bunkerHandshake: nil,
                resolvedProfiles: frames.resolvedProfiles)
        }
        // No identity domain sidecar this frame — return empty so the caller
        // knows to preserve the previous identity value. Any resolved_profiles
        // present in this frame are handled separately by `KernelModel.apply`
        // via `merging(resolvedProfiles:)`.
        return .empty
    }

    /// Return a copy of this identity with the given profiles merged in
    /// (additive — existing keys are preserved, new keys are inserted).
    /// Used by `KernelModel.apply` to deliver `resolved_profiles` on ticks
    /// where the identity domain sidecar is absent.
    func merging(resolvedProfiles newProfiles: [String: ResolvedProfile]) -> KernelIdentityProjection {
        guard !newProfiles.isEmpty else { return self }
        let merged = resolvedProfiles.merging(newProfiles) { _, new in new }
        return KernelIdentityProjection(
            activeAccount: activeAccount,
            activeNpub: activeNpub,
            accounts: accounts,
            bunkerHandshake: bunkerHandshake,
            resolvedProfiles: merged)
    }
}
