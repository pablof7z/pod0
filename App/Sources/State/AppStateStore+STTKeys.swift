import Foundation

// MARK: - AppStateStore STT key-presence sync
//
// The STT provider fallback policy lives in the Rust kernel (see
// `apps/nmp-app-podcast/src/store/stt_policy.rs`). Rust never holds the API
// keys — they live in the iOS Keychain — so the kernel can't read presence
// itself. This bridge reads the three cloud-STT credential stores and reports
// *which providers have a key* (never the secret) to the kernel via
// `podcast.settings.set_stt_keys_present`.
//
// The kernel mirrors the reported set verbatim and recomputes
// `settings.effectiveSttProvider` on the next snapshot tick. Callers must
// re-sync on launch (kernel attach) and after every STT key save/delete so the
// policy stays current.

extension AppStateStore {

    /// Read the cloud-STT credential stores and push the set of providers whose
    /// API key is present to the kernel. Reports the *full* present-set every
    /// time (the kernel replaces its mirror verbatim), so a deleted key clears
    /// correctly. A missing/empty key omits the provider. Apple on-device needs
    /// no key and is never included — the policy treats it as always available.
    func syncSTTKeysPresent() {
        var present: [String] = []
        if hasKey({ try ElevenLabsCredentialStore.apiKey() }) {
            present.append(STTProvider.elevenLabsScribe.rawValue)
        }
        if hasKey({ try OpenRouterCredentialStore.apiKey() }) {
            present.append(STTProvider.openRouterWhisper.rawValue)
        }
        if hasKey({ try AssemblyAICredentialStore.apiKey() }) {
            present.append(STTProvider.assemblyAI.rawValue)
        }
        kernel?.dispatch(namespace: "podcast.settings",
                         body: ["op": "set_stt_keys_present", "providers": present])
    }

    /// True when the credential lookup yields a non-empty key. Swallows the
    /// Keychain read error and reports "absent" — a transient read failure must
    /// not surface a provider as available when its key can't be read.
    private func hasKey(_ read: () throws -> String?) -> Bool {
        // `try?` flattens the `String?` return into a single optional level.
        guard let key = try? read(), !key.isEmpty else { return false }
        return true
    }
}
