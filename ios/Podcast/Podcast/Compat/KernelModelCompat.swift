// Compat shim — surviving surface for Nostr conversation/profile projections
// that have not yet landed on the Rust side. The `settings` projection has
// been migrated: read `model.snapshot?.settings.hasCompletedOnboarding` and
// write via `model.dispatch(namespace: "podcast", body: ["op": "update_settings", …])`.
//
// What remains: the in-memory `KernelState.nostrConversations` and
// `KernelState.nostrProfileCache` are still surfaced as compat empty state
// for Agent > Conversations (blocked on M7 — see `docs/BACKLOG.md`). The
// agent-trust surface (`pendingNostrApprovals`, `allowNostrPubkey`,
// `blockNostrPubkey`) is similarly still a no-op stub.

import Foundation

// MARK: - Kernel state (compat)

/// In-memory legacy-shaped state. Holds compat empty-state for projections
/// not yet migrated (Agent conversations + profile cache). The settings
/// projection moved to the Rust snapshot; read it via
/// `model.snapshot?.settings` and write via `podcast.update_settings`.
struct KernelState {
    /// Nostr conversation history surfaced by Agent > Conversations.
    /// Compat: always empty; populated when the Agent projection lands.
    var nostrConversations: [NostrConversationRecord] = []
    /// kind:0 profile metadata indexed by hex pubkey. Compat: always empty.
    var nostrProfileCache: [String: NostrProfileMetadata] = [:]
}

// MARK: - KernelModel surface area (compat)

extension KernelModel {

    /// Per-process in-memory state holder. Keyed by `ObjectIdentifier` so
    /// each `KernelModel` instance owns its own copy without leaking across
    /// previews/tests.
    @MainActor
    fileprivate static var compatStates: [ObjectIdentifier: KernelState] = [:]

    /// Legacy `state` accessor. Returns the in-memory compat state; never
    /// nil. Each `KernelModel` instance is keyed by identity so multiple
    /// instances (e.g. SwiftUI previews) do not share state.
    @MainActor
    var state: KernelState {
        get {
            let key = ObjectIdentifier(self)
            if let existing = Self.compatStates[key] { return existing }
            let fresh = KernelState()
            Self.compatStates[key] = fresh
            return fresh
        }
        set {
            Self.compatStates[ObjectIdentifier(self)] = newValue
        }
    }

    /// Called from `KernelModel.stop()` to remove the entry so the dictionary
    /// does not grow indefinitely when multiple instances are created (e.g.
    /// SwiftUI previews).
    @MainActor
    func cleanupCompatState() {
        Self.compatStates.removeValue(forKey: ObjectIdentifier(self))
    }


    /// Legacy podcast-lookup-by-feed. Compat: always returns nil. The
    /// onboarding subscribe flow will re-acquire this when the Rust kernel
    /// exposes a podcast projection.
    @MainActor
    func podcast(feedURL: URL) -> Podcast? { nil }

    /// Legacy subscription lookup. Compat: always returns nil.
    @MainActor
    func subscription(podcastID: UUID) -> Subscription? { nil }

    // MARK: - Nostr approvals (Agent surface)

    /// Pending trust-this-user approvals surfaced by `NostrApprovalPresenter`.
    /// Compat: always empty; the presenter never opens its sheet.
    @MainActor
    var pendingNostrApprovals: [NostrPendingApproval] { [] }

    /// Allows the pubkey hex on the trust list. Compat no-op.
    @MainActor
    func allowNostrPubkey(_ pubkeyHex: String) {}

    /// Blocks the pubkey hex. Compat no-op.
    @MainActor
    func blockNostrPubkey(_ pubkeyHex: String) {}
}

// MARK: - Subscription stub

/// Compat shim — replaced when subscription projection lands. Only used as
/// the return type of the always-nil `subscription(podcastID:)` shim, so the
/// fields are intentionally minimal.
struct Subscription: Hashable {
    var podcastID: UUID
}
