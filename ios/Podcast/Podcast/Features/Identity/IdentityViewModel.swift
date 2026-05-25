import Foundation
import os.log
import SwiftUI

private let identityVMLog = Logger(subsystem: "io.f7z.podcast", category: "IdentityViewModel")

// MARK: - IdentityMode
//
// Mirrors the `mode` discriminator surfaced by the Rust kernel via
// `AccountSummary.mode` in `apps/nmp-app-podcast/src/ffi/projections.rs`.
// The wire format is a string so the kernel can evolve the vocabulary
// without bumping `schema_version`; unknown values decode to `.none`.

enum IdentityMode: String, Sendable, Equatable {
    case none
    case localKey      = "local_key"
    case remoteSigner  = "remote_signer"

    /// Decode the kernel's `mode: String` payload. Unknown / empty values
    /// fall through to `.none` so an unrecognised mode never panics the UI.
    init(wire: String?) {
        guard let raw = wire?.trimmingCharacters(in: .whitespaces),
              !raw.isEmpty,
              let value = IdentityMode(rawValue: raw)
        else {
            self = .none
            return
        }
        self = value
    }
}

// MARK: - IdentityViewModel
//
// Pure value type that projects the active Nostr identity out of the
// kernel snapshot. Lives next to the Identity views because it owns the
// shape they read from — not a global, not `@Observable`.
//
// `KernelModel` is the single observed source: SwiftUI rebuilds the
// owning view when `podcastSnapshot` changes, and each rebuild
// reconstructs an `IdentityViewModel` from the latest snapshot. There is
// no derived caching, no separate observation, and no mutable state —
// reads stay constitutional (D2 / D8: snapshot-only).
//
// Mutation entry points live on the views themselves. They dispatch
// through `KernelModel.dispatch(namespace:body:)` once the kernel
// exposes identity actions; until then, each mutation surfaces a
// stable "actions land later" toast via [`stagedActionToast`] so the
// user always sees feedback even though the kernel can't honor the
// write yet.

struct IdentityViewModel {

    /// `nil` when the kernel has not yet populated an active account.
    /// `AccountSummary` is `Codable` but not `Equatable` in the generated
    /// bridge, so this struct doesn't synthesize `Equatable` either.
    /// Views observe individual derived projections (`displayName`,
    /// `pictureURLString`, etc.) which are `Equatable` on their own.
    let account: AccountSummary?

    /// Convenience: snapshot-derived in one call.
    init(snapshot: PodcastUpdate?) {
        self.account = snapshot?.activeAccount
    }

    init(account: AccountSummary?) {
        self.account = account
    }

    // MARK: - Derived projections

    var hasIdentity: Bool { account != nil }

    /// Full npub (Bech32-encoded pubkey). `nil` while no identity is loaded.
    var npub: String? { account?.npub }

    /// Shortened npub for compact rendering — first 10 + last 6 chars with
    /// a horizontal-ellipsis join. Matches the legacy stub's behaviour so
    /// existing views render identically once an identity is loaded.
    var npubShort: String? {
        guard let full = npub, full.count > 16 else { return npub }
        return "\(full.prefix(10))\u{2026}\(full.suffix(6))"
    }

    /// Display name from the kind-0 profile (relay-fetched). `nil` until
    /// the kernel has a profile cached. Views fall back to a generated
    /// slug via `UserProfileDisplay` when this is absent.
    var displayName: String? { account?.displayName }

    /// Raw picture-URL string from the kind-0 profile. Views resolve to
    /// `URL?` through `UserProfileDisplay.pictureURL` so the scheme guard
    /// stays in one place.
    var pictureURLString: String? { account?.pictureUrl }

    /// Signer flavour. `.none` when no identity is loaded *or* when the
    /// kernel emits an unrecognised mode string.
    var mode: IdentityMode { IdentityMode(wire: account?.mode) }

    var isRemoteSigner: Bool { mode == .remoteSigner }

    /// Stable copy surfaced when a mutation hits a kernel namespace that
    /// has not yet shipped (`identity.import_nsec`, `identity.publish_profile`,
    /// etc.). Tracked in `docs/BACKLOG.md` under the M1-exit identity-actions
    /// item. One sentence per the Identity-05 banner brief (§4.5).
    static let stagedActionToast =
        "Identity actions land with the M1-exit kernel update."
}

// MARK: - KernelModel projection helpers

extension KernelModel {

    /// Projected identity view — a pure read of `podcastSnapshot.activeAccount`.
    /// Views call this each render rather than caching it; SwiftUI re-runs
    /// the body whenever `podcastSnapshot` changes.
    var identity: IdentityViewModel {
        IdentityViewModel(snapshot: podcastSnapshot)
    }

    /// Stage-and-toast helper for identity mutations that don't have a
    /// kernel action wired yet. Surfaces the staged-action banner via the
    /// same `lastErrorToast` channel used by synchronous dispatch
    /// failures so the user sees one consistent feedback surface.
    ///
    /// `symbol` is the kernel-action id the call *would* dispatch to
    /// once it lands (e.g. `"identity.publish_profile"`). It's logged
    /// for diagnostics but not shown to the user.
    @MainActor
    func surfaceStagedIdentityAction(_ symbol: String) {
        identityVMLog.info("staged identity action: \(symbol, privacy: .public)")
        setErrorToast(IdentityViewModel.stagedActionToast)
    }
}
