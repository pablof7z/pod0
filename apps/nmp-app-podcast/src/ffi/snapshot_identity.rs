use super::handle::PodcastHandle;
use super::projections::AccountSummary;
use crate::store::identity::IdentityStore;
use std::fmt::Write;

/// Mode token for an account whose secret key lives inside this app's
/// [`IdentityStore`] (the `podcast.identity ImportNsec` / generate path).
const MODE_LOCAL_KEY: &str = "local_key";
/// Mode token for an external NIP-55 (Amber) signer: the kernel holds the
/// account, the private key never enters the process. The host renders this
/// verbatim into the identity badge.
const MODE_NIP55: &str = "nip55";

/// Build the active-account identity projection.
///
/// The kernel account manager is the single source of truth for *which*
/// account is active (V-82 — `NmpApp::active_account_handle()` exposes the
/// authoritative active-account pubkey slot the actor writes through on every
/// sign-in / account switch). A local-key sign-in (`ImportNsec`) and an
/// external NIP-55 / Amber sign-in (`nmp_app_signin_nip55`, which lands the
/// signer with `make_active = true`) both land in that slot.
///
/// The app-owned [`IdentityStore`] is **not** a competing source of truth: it
/// holds key material for the `local_key` path only (see `docs/BACKLOG.md`,
/// "social-bunker-signing-kernel" — `IdentityStore.secret_hex` covers
/// `.localKey` identities; remote signers are kernel-owned). It is consulted
/// here purely to enrich the `local_key` row with display name / picture and
/// to cover the cold pre-kernel-tick window (and unit tests) where the slot
/// has not yet been written.
///
/// Resolution order (no second source of truth — the kernel slot wins):
///
/// 1. Kernel active-account hex present **and** it equals the app-owned
///    local-key pubkey → render the enriched local-key row.
/// 2. Kernel active-account hex present but **not** the local-key pubkey (or
///    no local secret is held) → render a pubkey-only external (`nip55`) row:
///    the Amber account. `npub` is derived from the hex; no display/picture.
/// 3. Kernel slot empty → fall back to the app-owned `IdentityStore` (the
///    pre-first-tick local-key window).
pub(crate) fn build_active_account(handle: &PodcastHandle) -> Option<AccountSummary> {
    let kernel_active_hex = read_kernel_active_account(handle);
    let local = handle.state.library.identity.lock().ok();
    resolve_active_account(kernel_active_hex.as_deref(), local.as_deref())
}

/// The kernel's authoritative active-account pubkey (lowercase hex), or `None`
/// when no account is active.
///
/// Exposed so the `podcast.identity` typed projection can gate on the kernel
/// slot directly (the V-82 single source of truth), not only on the app-owned
/// `domain_revs.identity` counter. An external NIP-55 / Amber sign-in lands the
/// account by writing this slot from inside the kernel (`set_accounts` after
/// `AddSigner { make_active: true }`); it never touches the app's
/// `IdentityStore`, so the app-side rev counter never advances on that path.
/// Without a kernel-slot-aware gate the identity sidecar is omitted from the
/// very frame that carries the new account, and the host shows "Not signed in"
/// despite a successful sign-in (PR #417 propagation defect). This reader lets
/// the projection observe the slot transition with no polling — it is sampled
/// only when the closure already runs on a kernel-driven emit.
pub(crate) fn kernel_active_account_hex(handle: &PodcastHandle) -> Option<String> {
    read_kernel_active_account(handle)
}

/// Pure resolution policy shared by [`build_active_account`] and its tests:
/// given the kernel's authoritative active-account hex (if any) and the
/// app-owned local-key store (if lockable), decide the surfaced account. The
/// kernel slot wins; the local store only enriches a matching local key or
/// covers the pre-first-tick window. Extracted so there is ONE decision path
/// (no test-side mirror).
fn resolve_active_account(
    kernel_active_hex: Option<&str>,
    local: Option<&IdentityStore>,
) -> Option<AccountSummary> {
    match kernel_active_hex {
        Some(active_hex) => {
            // The kernel has named an active account. Decide whether it is the
            // app-owned local key (enrich) or an external signer (pubkey-only).
            let local_match = local
                .and_then(local_key_summary)
                .filter(|summary| summary.pubkey_hex == active_hex);
            match local_match {
                Some(summary) => Some(summary),
                None => external_account_summary(active_hex),
            }
        }
        // Pre-first-tick / kernel hasn't resolved an account yet: the only
        // account that can exist without a kernel write is the app-owned local
        // key (an external sign-in always writes the slot before it completes).
        None => local.and_then(local_key_summary),
    }
}

/// Read the kernel's authoritative active-account pubkey (lowercase hex), or
/// `None` if no account is active. Reads through `NmpApp::active_account_handle`
/// — the V-82 single-source-of-truth slot the actor writes on every sign-in.
///
/// SAFETY: `handle.app` is a non-null `*mut NmpApp` for the lifetime of the
/// `PodcastHandle` (constructed in `nmp_app_podcast_register`, freed only in
/// `nmp_app_podcast_unregister` after the kernel actor has joined — see the
/// `PodcastHandle` `Send`/`Sync` safety contract). The slot is an
/// `Arc<Mutex<Option<String>>>`; we clone the inner `String` out under the lock
/// and never retain the guard.
fn read_kernel_active_account(handle: &PodcastHandle) -> Option<String> {
    if handle.app.is_null() {
        return None;
    }
    // SAFETY: see doc comment — the pointer is valid and only read here.
    let app = unsafe { &*handle.app };
    let slot = app.active_account_handle();
    let guard = slot.lock().ok()?;
    guard.clone()
}

/// Project the app-owned [`IdentityStore`] into a `local_key` account summary,
/// or `None` when no canonical hex pubkey is held.
fn local_key_summary(id: &IdentityStore) -> Option<AccountSummary> {
    let npub = id.npub.as_ref()?;
    let pubkey_hex = id.pubkey_hex.as_ref()?;
    let fingerprint = account_fingerprint(pubkey_hex)?;
    Some(AccountSummary {
        npub: npub.clone(),
        pubkey_hex: pubkey_hex.clone(),
        fingerprint,
        mode: MODE_LOCAL_KEY.into(),
        display_name: id.display_name.clone(),
        picture_url: id.picture_url.clone(),
        name: id.name.clone(),
        about: id.about.clone(),
    })
}

/// Build a pubkey-only external (`nip55`) account summary from the kernel's
/// active-account hex. The private key lives in Amber, so there is no display
/// name / picture to enrich with here; `npub` is derived deterministically from
/// the hex. Returns `None` only if the hex fails to parse as a public key
/// (degrades silently per D6 rather than surfacing a malformed account).
fn external_account_summary(active_hex: &str) -> Option<AccountSummary> {
    let npub = npub_from_hex(active_hex)?;
    let fingerprint = account_fingerprint(active_hex)?;
    Some(AccountSummary {
        npub,
        pubkey_hex: active_hex.to_string(),
        fingerprint,
        mode: MODE_NIP55.into(),
        display_name: None,
        picture_url: None,
        name: None,
        about: None,
    })
}

/// Derive the bech32 `npub1…` encoding from a lowercase 64-hex pubkey, or
/// `None` if the hex is not a valid x-only public key. Uses `PublicKey::parse`
/// (the crate-wide accessor used by the comments/social/agent handlers); the
/// kernel slot always carries hex, which `parse` accepts.
fn npub_from_hex(hex: &str) -> Option<String> {
    use nostr::nips::nip19::ToBech32;
    let pubkey = nostr::PublicKey::parse(hex).ok()?;
    pubkey.to_bech32().ok()
}

/// Derive the stable short account fingerprint surfaced to every host.
///
/// The hash input is the decoded 32-byte Nostr public key, not the UTF-8 hex
/// string. That keeps the value independent of hex casing and aligned with
/// clients that fingerprint the key payload itself.
fn account_fingerprint(hex: &str) -> Option<String> {
    use sha2::{Digest, Sha256};

    let pubkey = nostr::PublicKey::parse(hex).ok()?;
    let digest = Sha256::digest(pubkey.as_bytes());
    let mut fingerprint = String::with_capacity("sha256:".len() + 16);
    fingerprint.push_str("sha256:");
    for byte in digest.iter().take(8) {
        write!(&mut fingerprint, "{byte:02x}").ok()?;
    }
    Some(fingerprint)
}

#[cfg(test)]
mod tests {
    use super::*;

    const TEST_NSEC: &str = "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";
    const TEST_PUBKEY_HEX: &str =
        "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";
    const TEST_NPUB: &str = "npub1cl6unlzp39qgdgha3sl9gtqade47avshtwjpsy778z7s9ymt6nls2thmtl";
    const TEST_FINGERPRINT: &str = "sha256:cf09e55b002a2b12";

    // The Amber test key proven on the emulator (see PR #417 evidence).
    const AMBER_PUBKEY_HEX: &str =
        "d6070609432b666c51677f606a0961e5f40730fe44b1c3bbd7ce29d5fa25b0a6";
    const AMBER_NPUB: &str = "npub16crsvz2r9dnxc5t80asx5ztpuh6qwv87gjcu8w7hec5at739kznqzxadlu";
    const AMBER_FINGERPRINT: &str = "sha256:72573f81ed78740b";

    fn local_store_with_key() -> IdentityStore {
        let mut identity = IdentityStore::new();
        identity.import_nsec(TEST_NSEC).expect("valid fixture key");
        identity.display_name = Some("Pod0 User".into());
        identity
    }

    // ---- local_key projection (app-owned IdentityStore) -------------------

    #[test]
    fn local_key_summary_projects_hex_and_npub() {
        let identity = local_store_with_key();
        let account = local_key_summary(&identity).expect("active account");
        assert_eq!(account.pubkey_hex, TEST_PUBKEY_HEX);
        assert_eq!(account.npub, TEST_NPUB);
        assert_eq!(account.fingerprint, TEST_FINGERPRINT);
        assert_eq!(account.display_name.as_deref(), Some("Pod0 User"));
        assert_eq!(account.mode, MODE_LOCAL_KEY);
    }

    #[test]
    fn local_key_summary_absent_without_canonical_hex() {
        let mut identity = IdentityStore::new();
        identity.npub = Some(TEST_NPUB.into());
        assert!(local_key_summary(&identity).is_none());
    }

    // ---- external (NIP-55 / Amber) projection -----------------------------

    #[test]
    fn external_account_surfaces_amber_pubkey_as_nip55() {
        // The kernel active-account slot holds an Amber pubkey with NO matching
        // local secret: the gap PR #417 was rejected for. It must now surface
        // as a pubkey-only `nip55` account, not "Not signed in".
        let account = external_account_summary(AMBER_PUBKEY_HEX).expect("external account");
        assert_eq!(account.pubkey_hex, AMBER_PUBKEY_HEX);
        assert_eq!(account.npub, AMBER_NPUB);
        assert_eq!(account.fingerprint, AMBER_FINGERPRINT);
        assert_eq!(account.mode, MODE_NIP55);
        assert!(account.display_name.is_none());
        assert!(account.picture_url.is_none());
    }

    #[test]
    fn external_account_none_for_malformed_hex() {
        assert!(external_account_summary("not-hex").is_none());
        assert!(external_account_summary("").is_none());
    }

    #[test]
    fn npub_from_hex_round_trips_amber_key() {
        assert_eq!(npub_from_hex(AMBER_PUBKEY_HEX).as_deref(), Some(AMBER_NPUB));
        assert_eq!(npub_from_hex(TEST_PUBKEY_HEX).as_deref(), Some(TEST_NPUB));
    }

    #[test]
    fn account_fingerprint_hashes_pubkey_bytes_not_hex_text() {
        assert_eq!(
            account_fingerprint(TEST_PUBKEY_HEX).as_deref(),
            Some(TEST_FINGERPRINT)
        );
        assert_eq!(
            account_fingerprint(&TEST_PUBKEY_HEX.to_uppercase()).as_deref(),
            Some(TEST_FINGERPRINT)
        );
        assert!(account_fingerprint("not-hex").is_none());
    }

    // ---- resolution policy (the real decision path) -----------------------
    //
    // These exercise `resolve_active_account` — the exact function
    // `build_active_account` delegates to (one decision path, no mirror). The
    // only part not covered here is the live raw-`NmpApp`-pointer read, which a
    // unit test cannot construct without a started kernel actor; that slot →
    // projection wiring is proven end-to-end on the emulator (PR #417 evidence)
    // and by NMP's own `active_account_handle` tests.

    #[test]
    fn kernel_active_amber_with_no_local_key_resolves_external() {
        let account = resolve_active_account(Some(AMBER_PUBKEY_HEX), None).expect("account");
        assert_eq!(account.pubkey_hex, AMBER_PUBKEY_HEX);
        assert_eq!(account.mode, MODE_NIP55);
    }

    #[test]
    fn kernel_active_amber_alongside_unrelated_local_key_prefers_amber() {
        // A stale local key in the store must NOT shadow the kernel's active
        // Amber account — the kernel slot wins.
        let local = local_store_with_key();
        let account =
            resolve_active_account(Some(AMBER_PUBKEY_HEX), Some(&local)).expect("account");
        assert_eq!(account.pubkey_hex, AMBER_PUBKEY_HEX);
        assert_eq!(account.mode, MODE_NIP55);
        // Display name from the unrelated local store must not leak onto it.
        assert!(account.display_name.is_none());
    }

    #[test]
    fn kernel_active_matches_local_key_enriches_with_display() {
        let local = local_store_with_key();
        let account = resolve_active_account(Some(TEST_PUBKEY_HEX), Some(&local)).expect("account");
        assert_eq!(account.pubkey_hex, TEST_PUBKEY_HEX);
        assert_eq!(account.mode, MODE_LOCAL_KEY);
        assert_eq!(account.display_name.as_deref(), Some("Pod0 User"));
    }

    #[test]
    fn empty_kernel_slot_falls_back_to_local_key() {
        let local = local_store_with_key();
        let account = resolve_active_account(None, Some(&local)).expect("account");
        assert_eq!(account.pubkey_hex, TEST_PUBKEY_HEX);
        assert_eq!(account.mode, MODE_LOCAL_KEY);
    }

    #[test]
    fn empty_kernel_slot_and_no_local_key_resolves_none() {
        assert!(resolve_active_account(None, None).is_none());
        let empty = IdentityStore::new();
        assert!(resolve_active_account(None, Some(&empty)).is_none());
    }
}
