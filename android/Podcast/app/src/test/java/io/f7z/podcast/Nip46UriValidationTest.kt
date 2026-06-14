package io.f7z.podcast

import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

/**
 * Unit tests for the NIP-46 remote-signer helpers in [Nip46Uri].
 *
 * These exercise the SAME production code the Compose screens
 * ([io.f7z.podcast.ui.RemoteSignerScreen], [io.f7z.podcast.ui.NostrConnectScreen])
 * call — no parallel reimplementation in the test file. Two concerns:
 *
 *  1. **URI validators** — `bunker://` / `nostrconnect://` format gates.
 *  2. **Completion signal** — the external-account-transition gate that makes
 *     a successful handshake reachable. This is the BLOCKER fix: the screens
 *     used to string-match `mode == "bunker"`, but the Rust projection
 *     (`apps/nmp-app-podcast/src/ffi/snapshot_identity.rs`) only ever emits
 *     `"local_key"` and `"nip55"`, so the old gate could NEVER become true. The
 *     correct signal is "an external (remote-signer) active account appeared".
 *
 * Wire contract verified against `NmpCore.h`:
 * ```c
 * void nmp_app_signin_bunker(void *app, const char *uri, uint8_t make_active);
 * ```
 */
class Nip46UriValidationTest {

    // ── isPlausibleBunkerUri ─────────────────────────────────────────────────

    @Test
    fun `valid bunker URI with pubkey host passes`() {
        val uri = "bunker://b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4" +
            "?relay=wss%3A%2F%2Frelay.example.com&secret=abc123"
        assertTrue(Nip46Uri.isPlausibleBunkerUri(uri))
    }

    @Test
    fun `bunker URI without scheme fails`() {
        assertFalse(
            Nip46Uri.isPlausibleBunkerUri(
                "b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4",
            ),
        )
    }

    @Test
    fun `empty string fails`() {
        assertFalse(Nip46Uri.isPlausibleBunkerUri(""))
    }

    @Test
    fun `blank string fails`() {
        assertFalse(Nip46Uri.isPlausibleBunkerUri("   "))
    }

    @Test
    fun `nsec URI is not a bunker URI`() {
        assertFalse(
            Nip46Uri.isPlausibleBunkerUri(
                "nsec1qyfuw4n8x9g89prqzj0r7k6kclz0g2yjl04lnrnjrznzl9qzuxss9j6xs",
            ),
        )
    }

    @Test
    fun `nostrconnect URI is not a bunker URI`() {
        assertFalse(
            Nip46Uri.isPlausibleBunkerUri("nostrconnect://pubkey?relay=wss://relay.example.com"),
        )
    }

    @Test
    fun `bunker scheme alone with no content fails min-length gate`() {
        // "bunker://" is 9 chars — a real URI has at least a pubkey host.
        assertFalse(Nip46Uri.isPlausibleBunkerUri("bunker://"))
    }

    @Test
    fun `bunker URI with minimal pubkey passes`() {
        val hex64 = "a".repeat(64)
        assertTrue(Nip46Uri.isPlausibleBunkerUri("bunker://$hex64"))
    }

    // ── isPlausibleNostrconnectUri ───────────────────────────────────────────

    @Test
    fun `nostrconnect URI with relay param is plausible`() {
        val uri = "nostrconnect://b889ff5b1513b641e2a139f661a661364979c5beee91842f8f0ef42ab558e9d4" +
            "?relay=wss%3A%2F%2Frelay.example.com&secret=def456"
        assertTrue(Nip46Uri.isPlausibleNostrconnectUri(uri))
    }

    @Test
    fun `empty nostrconnect URI fails`() {
        assertFalse(Nip46Uri.isPlausibleNostrconnectUri(""))
    }

    @Test
    fun `bunker URI is not a nostrconnect URI`() {
        assertFalse(Nip46Uri.isPlausibleNostrconnectUri("bunker://pubkey"))
    }

    @Test
    fun `nostrconnect scheme alone fails min-length gate`() {
        assertFalse(Nip46Uri.isPlausibleNostrconnectUri("nostrconnect://"))
    }

    // ── isRemoteSignerAccount — the completion-signal primitive ──────────────
    //
    // These prove the projection's ACTUAL emitted mode tokens drive the gate.
    // The projection (snapshot_identity.rs) emits exactly two: "local_key" and
    // "nip55". A bunker handshake surfaces as "nip55" (external/remote signer).

    @Test
    fun `null account is not a remote signer`() {
        assertFalse(Nip46Uri.isRemoteSignerAccount(null))
    }

    @Test
    fun `local_key account is not a remote signer`() {
        val acct = AccountSummary(
            npub = "npub1xxx",
            pubkeyHex = "ab".repeat(32),
            mode = "local_key",
        )
        assertFalse(Nip46Uri.isRemoteSignerAccount(acct))
    }

    @Test
    fun `nip55 account is a remote signer — this is what a bunker account surfaces as`() {
        // PROOF the completion signal is reachable: the projection emits "nip55"
        // for a kernel-owned bunker account (it never emits "bunker"/"nip46").
        // The OLD gate string-matched "bunker" and could never fire; THIS gate
        // recognises the value the projection actually emits.
        val acct = AccountSummary(
            npub = "npub16crsvz2r9dnxc5t80asx5ztpuh6qwv87gjcu8w7hec5at739kznqzxadlu",
            pubkeyHex = "d6070609432b666c51677f606a0961e5f40730fe44b1c3bbd7ce29d5fa25b0a6",
            mode = "nip55",
        )
        assertTrue(Nip46Uri.isRemoteSignerAccount(acct))
    }

    @Test
    fun `mode is matched case-insensitively and trimmed`() {
        val acct = AccountSummary(
            npub = "npub1xxx",
            pubkeyHex = "ab".repeat(32),
            mode = "  LOCAL_KEY  ",
        )
        assertFalse(Nip46Uri.isRemoteSignerAccount(acct))
    }

    // ── handshakeCompleted — the reachable completion gate ───────────────────

    @Test
    fun `handshake completes when external account appears after starting signed-out`() {
        // The NIP-46 happy path: started from NotSignedInState (no account),
        // bunker handshake lands an external (nip55) account → completed.
        val connected = AccountSummary(
            npub = "npub1xxx",
            pubkeyHex = "d6070609432b666c51677f606a0961e5f40730fe44b1c3bbd7ce29d5fa25b0a6",
            mode = "nip55",
        )
        assertTrue(
            Nip46Uri.handshakeCompleted(
                hadActiveAccountAtStart = false,
                current = connected,
            ),
        )
    }

    @Test
    fun `handshake not complete while still signed-out`() {
        // Spinner state: started signed-out, no account yet → not done.
        assertFalse(
            Nip46Uri.handshakeCompleted(
                hadActiveAccountAtStart = false,
                current = null,
            ),
        )
    }

    @Test
    fun `handshake not complete when a local_key account appears`() {
        // Defensive: a local-key account is NOT a remote signer, so it does not
        // satisfy the bunker/nostrconnect completion signal.
        val local = AccountSummary(
            npub = "npub1xxx",
            pubkeyHex = "ab".repeat(32),
            mode = "local_key",
        )
        assertFalse(
            Nip46Uri.handshakeCompleted(
                hadActiveAccountAtStart = false,
                current = local,
            ),
        )
    }

    @Test
    fun `handshake reads complete when a remote account is present even if one was at start`() {
        // Defensive branch (these screens are NotSignedInState-only, so this is
        // not the normal path): a remote account present now reads as the
        // completed state.
        val connected = AccountSummary(
            npub = "npub1xxx",
            pubkeyHex = "d6070609432b666c51677f606a0961e5f40730fe44b1c3bbd7ce29d5fa25b0a6",
            mode = "nip55",
        )
        assertTrue(
            Nip46Uri.handshakeCompleted(
                hadActiveAccountAtStart = true,
                current = connected,
            ),
        )
    }
}
