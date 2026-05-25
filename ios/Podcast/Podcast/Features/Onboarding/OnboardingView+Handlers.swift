import SwiftUI

// MARK: - OnboardingView per-step handlers
//
// Extracted from `OnboardingView.swift` so the main view stays focused on
// layout and state management. Each handler advances the step after running
// any side effects (Keychain writes, BYOK connect calls, etc.).
//
// NMP migration note: `finishOnboarding()` now writes the
// `hasCompletedOnboarding` flag through the Rust `podcast.update_settings`
// action — the kernel `PodcastStore` is authoritative and persists across
// launches. Identity name / picture still seed `agent.profile.*`
// UserDefaults keys read by `AgentIdentityView` (PR 11); BYOK / manual
// OpenRouter credential persistence still rides on `OpenRouterCredentialStore`
// + Keychain until the LLM-provider capability lands (see
// `docs/BACKLOG.md` — "M3 — Settings projection" for the remaining shadows).

extension OnboardingView {

    func handleAISetupContinue() {
        let trimmed = apiKeyDraft.trimmed
        guard !trimmed.isEmpty else {
            apiKeyError = nil
            advance()
            return
        }
        apiKeySaving = true
        apiKeyError = nil
        do {
            try OpenRouterCredentialStore.saveAPIKey(trimmed)
            // The kernel `settings` projection has no `openRouterMode` field
            // yet — `OpenRouterCredentialStore` (Keychain) is still the only
            // persisted side-effect of saving a manual key. See
            // `docs/BACKLOG.md` — "M3 — Settings projection" for the
            // remaining LLM-provider credential surface.
            apiKeyDraft = ""
            apiKeySaving = false
            Haptics.success()
            advance()
        } catch {
            apiKeySaving = false
            apiKeyError = "Could not save key. Tap Skip or try again."
            Haptics.error()
        }
    }

    func handleBYOKConnect() async {
        isConnectingBYOK = true
        apiKeyError = nil
        defer { isConnectingBYOK = false }
        do {
            let tokens = try await byokConnect.connectPodcastProviders()
            // BYOK tokens are still consumed by the Swift-side credential
            // store (Keychain). The kernel `settings` projection has no
            // BYOK-import surface yet — see `docs/BACKLOG.md`
            // — "M3 — Settings projection" for the remaining shadow.
            guard !tokens.isEmpty else {
                throw BYOKConnectError.noProviderKeysReturned
            }
            apiKeyDraft = ""
            Haptics.success()
            advance()
        } catch BYOKConnectError.cancelled {
            // user cancelled — no error shown
        } catch {
            apiKeyError = error.localizedDescription
            Haptics.error()
        }
    }

    func handleIdentityContinue() {
        // Profile name + picture seed the same UserDefaults keys
        // (`agent.profile.name`, `agent.profile.pictureURL`) that
        // `AgentIdentityView` reads via `@AppStorage`. This avoids
        // round-tripping through the in-memory compat `Settings` struct,
        // which dropped values across launches. The keys are namespaced
        // for the M3 settings-projection migration (see `docs/BACKLOG.md`).
        let nameTrimmed = agentNameDraft.trimmed
        let pictureTrimmed = profilePictureDraft.trimmed
        let defaults = UserDefaults.standard
        if !nameTrimmed.isEmpty {
            defaults.set(nameTrimmed, forKey: "agent.profile.name")
        }
        if !pictureTrimmed.isEmpty {
            defaults.set(pictureTrimmed, forKey: "agent.profile.pictureURL")
        }
        Haptics.success()
        advance()
    }

    func finishOnboarding() {
        model.dispatch(
            namespace: "podcast",
            body: [
                "op": "update_settings",
                "has_completed_onboarding": true,
            ]
        )
        Haptics.success()
    }
}
