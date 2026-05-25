// Compat shim — replaced when the settings projection lands in nmp-app-podcast.
//
// `Settings` is the legacy domain struct mutated through `state.settings`.
// Only the fields read or written by the migrated Identity / Onboarding /
// Agent views are included; the legacy struct has dozens more fields that
// will return when settings ship as a kernel projection.
//
// Equatable conformance is required because `AgentIdentityView` uses
// `.onChange(of: settings)` to write changes back via `store.updateSettings`.

import Foundation

struct Settings: Equatable, Hashable {
    // Onboarding flag — read in OnboardingView; written in OnboardingView+Handlers.
    var hasCompletedOnboarding: Bool = false

    // Nostr identity / agent profile fields used by Agent views + Onboarding.
    var nostrProfileName: String = ""
    var nostrProfileAbout: String = ""
    var nostrProfilePicture: String = ""
    var nostrPublicKeyHex: String?
    var nostrRelayURL: String = "wss://relay.tenex.chat"

    init() {}

    // MARK: - Mutating helpers

    /// Legacy helper that flipped the OpenRouter credential source after a
    /// manual key save. Compat shim: no-op (credential source not yet
    /// surfaced in the compat struct).
    mutating func markOpenRouterManual() {}
}
