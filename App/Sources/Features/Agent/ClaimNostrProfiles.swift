import SwiftUI

// MARK: - Reference-first profile claiming
//
// Replaces the bespoke `NostrProfileFetcher` websocket. A view that displays
// Nostr profiles attaches `.claimNostrProfiles(_:consumer:)` with the pubkeys
// it renders; the kernel resolves each one over its own relay pool and
// delivers the result through `projections.resolved_profiles`, which
// `AppStateStore.mergeResolvedProfiles` folds into `nostrProfileCache`. The
// display reads from that cache and re-renders reactively when it lands — no
// host-opened socket.
//
// The kernel refcounts each claim per `consumer_id`; the modifier reconciles
// the claimed set whenever `pubkeys` changes (claim the newly-added, release
// the no-longer-present) and releases everything on disappear. `consumer` is a
// stable per-surface token so two surfaces displaying the same pubkey don't
// stomp each other's refcount. Claims are bounded kernel-side (drop-newest
// over `MAX_CLAIMS_PER_PUBKEY`), so an imperfect release degrades gracefully.

private struct ClaimNostrProfilesModifier: ViewModifier {
    let pubkeys: Set<String>
    let consumer: String

    @State private var claimed: Set<String> = []

    func body(content: Content) -> some View {
        content
            .onAppear { reconcile(to: pubkeys) }
            .onDisappear { reconcile(to: []) }
            .onChange(of: pubkeys) { _, next in reconcile(to: next) }
    }

    /// Diff `claimed` against `target`: claim the additions, release the
    /// removals. Idempotent — re-running with the same target is a no-op.
    private func reconcile(to target: Set<String>) {
        guard let kernel = KernelModel.shared else { return }
        let valid = target.filter { !$0.isEmpty }
        for pubkey in valid.subtracting(claimed) {
            kernel.claimProfile(pubkeyHex: pubkey, consumerID: consumer)
        }
        for pubkey in claimed.subtracting(valid) {
            kernel.releaseProfile(pubkeyHex: pubkey, consumerID: consumer)
        }
        claimed = valid
    }
}

extension View {
    /// Claim kind:0 profiles for `pubkeys` while this view is on screen so they
    /// resolve through the kernel's `resolved_profiles` push into
    /// `nostrProfileCache`. `consumer` must be a stable per-surface token.
    func claimNostrProfiles(_ pubkeys: Set<String>, consumer: String) -> some View {
        modifier(ClaimNostrProfilesModifier(pubkeys: pubkeys, consumer: consumer))
    }
}
