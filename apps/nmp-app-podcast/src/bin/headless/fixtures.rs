//! Hardcoded test fixtures for the headless scenario binary.
//!
//! Keypair generated once with `nak key generate` and baked in for
//! reproducibility across CI runs. The secret is never used for real Nostr
//! operations — it exists solely to seed an identity inside the headless app.

/// Hex-encoded 32-byte secret key used for all headless test runs.
#[allow(dead_code)]
pub const HEADLESS_TEST_SECRET_HEX: &str =
    "c34df03f16b033cef2e2075bce2363905dcb47087023eca55360a593f56ad7dc";

/// Corresponding hex-encoded public key.
#[allow(dead_code)]
pub const HEADLESS_TEST_PUBKEY_HEX: &str =
    "c7f5c9fc41894086a2fd8c3e542c1d6e6beeb2175ba41813de38bd02936bd4ff";

/// bech32-encoded `nsec1…` form of `HEADLESS_TEST_SECRET_HEX`.
/// Generated with `nak encode nsec c34df03f…`.
#[allow(dead_code)]
pub const HEADLESS_TEST_NSEC: &str =
    "nsec1cdxlq0ckkqeuauhzqaduugmrjpwuk3cgwq37ef2nvzje8at26lwqapk9us";

/// Expected bech32 `npub1…` for the headless test pubkey.
#[allow(dead_code)]
pub const HEADLESS_TEST_NPUB: &str =
    "npub1cl6unlzp39qgdgha3sl9gtqade47avshtwjpsy778z7s9ymt6nls2thmtl";
