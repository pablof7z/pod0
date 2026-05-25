// Compat shim — replaced when the kernel snapshot exposes podcast/subscription
// state (M2) and settings projection (M3+).
//
// The migrated Onboarding + Settings/Agent views were authored against the
// legacy `AppStateStore`, which surfaced `state.settings`, `updateSettings`,
// `podcast(feedURL:)`, and `subscription(podcastID:)`. The new KernelModel
// from M0.B has none of those yet.
//
// This file adds them as extension members backed by an in-memory
// `KernelState` that lives outside the Rust snapshot. The store will be
// authoritative once the matching projection modules land in `nmp-app-podcast`.

import Foundation

// MARK: - Kernel state (compat)

/// In-memory legacy-shaped state. Replaced when the Rust kernel snapshot
/// gains projection-level settings + podcast + subscription tables.
struct KernelState {
    var settings: Settings = Settings()
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
    private static var compatStates: [ObjectIdentifier: KernelState] = [:]

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

    /// Legacy settings-update method. Compat: assigns to the in-memory
    /// state. Persistence will be re-introduced when settings projection
    /// lands in the Rust kernel.
    @MainActor
    func updateSettings(_ newSettings: Settings) {
        var snapshot = state
        snapshot.settings = newSettings
        state = snapshot
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

// MARK: - AutoDownloadPolicy

struct AutoDownloadPolicy: Hashable, Codable {
    enum Mode: Hashable, Codable {
        case off
        case latestN(Int)
        case allNew
    }
    var mode: Mode = .latestN(5)
    var wifiOnly: Bool = true

    static let `default` = AutoDownloadPolicy()
}

// MARK: - Subscription stub

/// Compat shim — replaced when subscription projection lands.
struct Subscription: Hashable {
    var podcastID: UUID
    var notificationsEnabled: Bool = true
    var autoDownload: AutoDownloadPolicy = .default
}
