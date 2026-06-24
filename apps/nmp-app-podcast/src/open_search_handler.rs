//! Open search handler — routes Nostr-facing text input through NMP's
//! input-intent classifier and dispatch framework.
//!
//! Issue #605: Move all Nostr protocol detection from native shells into
//! the kernel via NMP's framework-level APIs. Eliminates ad-hoc bech32
//! parsing (npub, nprofile, nevent) and NIP-05 resolution in Swift/Kotlin.
//!
//! **Blocked on NMP #597:** The new NMP rev (1e1445973, 382 commits ahead of
//! v0.7.2) exposes `open_search` action, `InputIntent` enum variants, NIP-50
//! relay targeting, and kind:10007 preference types. This module is a
//! structural placeholder — implementation awaits those NMP APIs.
//!
//! **Current state (Phase 0-1):** Raw Nostr identifier and NIP-05 detection
//! with placeholder handlers. iOS AddByURLForm and NostrDiscoverForm check
//! `NostrNpub.looksLikeNostrInput()` to route candidates to `kernelNostrOpenSearch()`,
//! which dispatches `PodcastAction::OpenSearch` to the kernel.

use serde_json::json;

/// Detect if input looks like a Nostr private key (nsec1 prefix).
/// These must never be routed to open_search — callers should reject them
/// immediately with a user-visible warning.
pub(crate) fn looks_like_nsec_key(input: &str) -> bool {
    input.starts_with("nsec1")
}

/// Detect if input looks like a public Nostr identifier (npub/nprofile/nevent prefix).
/// Does NOT match nsec1 — private keys are handled separately via `looks_like_nsec_key`.
pub(crate) fn looks_like_nostr_identifier(input: &str) -> bool {
    input.starts_with("npub1")
        || input.starts_with("nprofile1")
        || input.starts_with("nevent1")
}

/// Detect if input looks like a NIP-05 address (user@domain.com, not http://).
pub(crate) fn looks_like_nip05_address(input: &str) -> bool {
    input.contains('@')
        && !input.starts_with("http://")
        && !input.starts_with("https://")
        && input.contains('.')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_looks_like_nostr_identifier_npub() {
        assert!(looks_like_nostr_identifier("npub1xyz"));
    }

    #[test]
    fn test_looks_like_nostr_identifier_nprofile() {
        assert!(looks_like_nostr_identifier("nprofile1xyz"));
    }

    #[test]
    fn test_looks_like_nostr_identifier_nevent() {
        assert!(looks_like_nostr_identifier("nevent1xyz"));
    }

    #[test]
    fn test_looks_like_nip05_address() {
        assert!(looks_like_nip05_address("user@example.com"));
    }

    #[test]
    fn test_nip05_rejects_http_urls() {
        assert!(!looks_like_nip05_address("http://example.com"));
    }

    #[test]
    fn test_nip05_rejects_https_urls() {
        assert!(!looks_like_nip05_address("https://example.com"));
    }

    #[test]
    fn test_looks_like_nostr_identifier_rejects_nsec() {
        // nsec1 is a private key — must NOT match as a public identifier
        assert!(!looks_like_nostr_identifier("nsec1abc"));
    }

    #[test]
    fn test_looks_like_nsec_key() {
        assert!(looks_like_nsec_key("nsec1abc"));
    }

    #[test]
    fn test_looks_like_nsec_key_rejects_npub() {
        assert!(!looks_like_nsec_key("npub1abc"));
    }

    #[test]
    fn test_nip05_rejects_no_dot() {
        assert!(!looks_like_nip05_address("user@localhost"));
    }

    #[test]
    fn test_nip05_rejects_no_at() {
        assert!(!looks_like_nip05_address("example.com"));
    }
}
