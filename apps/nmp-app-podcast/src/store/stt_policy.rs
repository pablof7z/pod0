//! Kernel-owned STT provider fallback policy.
//!
//! This is business policy that used to live in Swift
//! (`TranscriptIngestService.effectiveSTTProvider` / `resolvedSTTKey`). It
//! decides which speech-to-text provider actually runs given the user's
//! selection and which API keys are present in the Keychain.
//!
//! The rule (ported verbatim from the Swift implementation):
//!   - `apple_native` is always available (on-device, no key) → it always wins.
//!   - A provider that *requires* a key, but whose key is absent, downgrades
//!     to `apple_native`.
//!   - Otherwise the selected provider runs unchanged.
//!
//! Rust never holds the secret. Swift reads the Keychain and reports the set
//! of providers whose key is present via
//! `podcast.settings.set_stt_keys_present`; this policy reads that set.

/// STT-provider raw value for Apple's on-device `SpeechTranscriber`.
/// No API key required — always available, so it is the universal fallback.
pub const APPLE_NATIVE: &str = "apple_native";
const ELEVENLABS_SCRIBE: &str = "elevenlabs_scribe";
const ASSEMBLYAI: &str = "assemblyai";
const OPENROUTER_WHISPER: &str = "openrouter_whisper";

/// Whether `provider` (an STT-provider raw value) needs an API key to run.
///
/// `apple_native` is the only key-free provider. Any *unknown* raw value is
/// treated as key-requiring so a future cloud provider added on the Swift
/// side fails safe (it will fall back to `apple_native` until the key signal
/// is wired) rather than silently running without a key.
pub fn requires_key(provider: &str) -> bool {
    provider != APPLE_NATIVE
}

/// Resolve the effective STT provider from the user's selection and the set
/// of providers whose key is present in the Keychain.
///
/// Mirrors Swift `TranscriptIngestService.effectiveSTTProvider`:
///   - `apple_native` always wins (no key needed).
///   - A key-requiring provider with a present key stays selected.
///   - A key-requiring provider with an absent key downgrades to `apple_native`.
///
/// `keys_present` holds STT-provider raw values (e.g. `"elevenlabs_scribe"`).
/// Returns a `'static` raw value so callers can hand it straight to the
/// snapshot projection without an allocation.
pub fn effective_stt_provider<S>(selected: &str, keys_present: &S) -> &'static str
where
    S: KeyPresence + ?Sized,
{
    match selected {
        ELEVENLABS_SCRIBE => {
            if keys_present.contains(ELEVENLABS_SCRIBE) {
                ELEVENLABS_SCRIBE
            } else {
                APPLE_NATIVE
            }
        }
        ASSEMBLYAI => {
            if keys_present.contains(ASSEMBLYAI) {
                ASSEMBLYAI
            } else {
                APPLE_NATIVE
            }
        }
        OPENROUTER_WHISPER => {
            if keys_present.contains(OPENROUTER_WHISPER) {
                OPENROUTER_WHISPER
            } else {
                APPLE_NATIVE
            }
        }
        // `apple_native` and any unrecognised value resolve to the always-
        // available on-device provider.
        _ => APPLE_NATIVE,
    }
}

/// Abstraction over "is this provider's key present?" so the policy can be
/// tested against a plain slice while production code passes the store's
/// `BTreeSet<String>`.
pub trait KeyPresence {
    fn contains(&self, provider: &str) -> bool;
}

impl KeyPresence for std::collections::BTreeSet<String> {
    fn contains(&self, provider: &str) -> bool {
        std::collections::BTreeSet::contains(self, provider)
    }
}

impl KeyPresence for [&str] {
    fn contains(&self, provider: &str) -> bool {
        self.iter().any(|p| *p == provider)
    }
}

impl super::PodcastStore {
    /// Replace the set of STT providers whose API key is present in the
    /// Keychain. `providers` is a list of STT-provider raw values
    /// (`"elevenlabs_scribe"`, `"assemblyai"`, `"openrouter_whisper"`).
    /// Swift reads the Keychain and reports the full present-set on launch and
    /// whenever a key is saved or deleted; the kernel mirrors it verbatim.
    /// Not persisted — re-synced from the Keychain on every app launch.
    pub fn set_stt_keys_present(&mut self, providers: Vec<String>) {
        self.stt_keys_present = providers.into_iter().collect();
    }

    /// Whether the API key for `provider` (an STT-provider raw value) is
    /// currently present in the Keychain, per Swift's last report.
    pub fn stt_key_present(&self, provider: &str) -> bool {
        self.stt_keys_present.contains(provider)
    }

    /// The kernel-owned effective STT provider: the policy that decides which
    /// provider actually runs given the user's selection and which keys are
    /// present. See [`effective_stt_provider`].
    ///
    /// Returns the resolved provider raw value (one of `"apple_native"`,
    /// `"elevenlabs_scribe"`, `"assemblyai"`, `"openrouter_whisper"`).
    pub fn effective_stt_provider(&self) -> &'static str {
        effective_stt_provider(&self.stt_provider, &self.stt_keys_present)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn present(items: &[&str]) -> std::collections::BTreeSet<String> {
        items.iter().map(|s| (*s).to_owned()).collect()
    }

    #[test]
    fn apple_native_always_wins_even_with_no_keys() {
        let keys = present(&[]);
        assert_eq!(effective_stt_provider("apple_native", &keys), "apple_native");
    }

    #[test]
    fn apple_native_wins_even_when_other_keys_present() {
        let keys = present(&["elevenlabs_scribe", "assemblyai", "openrouter_whisper"]);
        assert_eq!(effective_stt_provider("apple_native", &keys), "apple_native");
    }

    #[test]
    fn elevenlabs_falls_back_when_key_absent() {
        let keys = present(&[]);
        assert_eq!(
            effective_stt_provider("elevenlabs_scribe", &keys),
            "apple_native"
        );
    }

    #[test]
    fn elevenlabs_stays_selected_when_key_present() {
        let keys = present(&["elevenlabs_scribe"]);
        assert_eq!(
            effective_stt_provider("elevenlabs_scribe", &keys),
            "elevenlabs_scribe"
        );
    }

    #[test]
    fn assemblyai_falls_back_when_key_absent() {
        let keys = present(&["elevenlabs_scribe"]); // unrelated key present
        assert_eq!(effective_stt_provider("assemblyai", &keys), "apple_native");
    }

    #[test]
    fn assemblyai_stays_selected_when_key_present() {
        let keys = present(&["assemblyai"]);
        assert_eq!(effective_stt_provider("assemblyai", &keys), "assemblyai");
    }

    #[test]
    fn openrouter_whisper_falls_back_when_key_absent() {
        let keys = present(&[]);
        assert_eq!(
            effective_stt_provider("openrouter_whisper", &keys),
            "apple_native"
        );
    }

    #[test]
    fn openrouter_whisper_stays_selected_when_key_present() {
        let keys = present(&["openrouter_whisper"]);
        assert_eq!(
            effective_stt_provider("openrouter_whisper", &keys),
            "openrouter_whisper"
        );
    }

    #[test]
    fn unknown_provider_resolves_to_apple_native() {
        let keys = present(&["some_future_provider"]);
        assert_eq!(
            effective_stt_provider("some_future_provider", &keys),
            "apple_native"
        );
    }

    #[test]
    fn requires_key_only_false_for_apple_native() {
        assert!(!requires_key("apple_native"));
        assert!(requires_key("elevenlabs_scribe"));
        assert!(requires_key("assemblyai"));
        assert!(requires_key("openrouter_whisper"));
        assert!(requires_key("anything_else"));
    }

    #[test]
    fn slice_key_presence_matches_btreeset() {
        let slice: &[&str] = &["assemblyai"];
        assert_eq!(effective_stt_provider("assemblyai", slice), "assemblyai");
        assert_eq!(
            effective_stt_provider("elevenlabs_scribe", slice),
            "apple_native"
        );
    }
}
