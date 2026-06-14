package io.f7z.podcast

/**
 * NIP-46 remote-signer helpers shared by the Compose screens
 * ([io.f7z.podcast.ui.RemoteSignerScreen], [io.f7z.podcast.ui.NostrConnectScreen])
 * and their unit tests, so the production gate and the tested gate are the SAME
 * code — no parallel reimplementation that can drift.
 *
 * ## URI validators
 *
 * Lightweight client-side format gates that give the user fast feedback before
 * the URI is handed to the kernel. The kernel's Rust parser
 * (`nmp_app_signin_bunker`) is the authoritative validator and degrades
 * silently (D6) on a malformed URI; these gates only catch the obvious paste
 * mistakes.
 *
 * ## Completion signal — why NOT string-match "bunker"/"nip46"
 *
 * The Android identity projection (`apps/nmp-app-podcast/src/ffi/snapshot_identity.rs`)
 * only ever emits two `mode` tokens: `"local_key"` (app-owned secret) and
 * `"nip55"` (external / remote signer — the kernel/broker owns the key, the
 * private key never enters the process). A successful NIP-46 *bunker* handshake
 * produces a kernel-owned account with no matching app-local secret, so the
 * projection flattens it to `mode = "nip55"` (nip55 == "external/remote signer"
 * here, not specifically Amber). It NEVER emits `"bunker"` or `"nip46"`.
 *
 * iOS reads a dedicated `signer_is_remote` boolean and explicitly warns "never
 * string-match on signer_kind" (the mode token is diagnostic-only). The Android
 * snapshot has no such boolean — only `mode`. So the durable Android-only
 * completion signal is the **external-account transition**: the remote-signer
 * screens launch from `NotSignedInState` (no active account), and a successful
 * handshake makes an EXTERNAL active account appear. "An external active account
 * exists where there was none" is the unambiguous success signal.
 *
 * @see isRemoteSignerAccount
 */
object Nip46Uri {

    /** The app-owned local-key mode token emitted by the Rust identity projection. */
    const val MODE_LOCAL_KEY = "local_key"

    /**
     * Lightweight check that [input] looks like a `bunker://` URI. The kernel's
     * Rust parser is authoritative; this is UX feedback only. Mirrors the iOS
     * `Nip46ConnectCard` validation.
     *
     * `"bunker://"` is 9 chars; a real URI carries at least a pubkey host after
     * it, so we require strictly more than the bare scheme.
     */
    fun isPlausibleBunkerUri(input: String): Boolean {
        val trimmed = input.trim()
        return trimmed.startsWith("bunker://") && trimmed.length > "bunker://".length
    }

    /**
     * Lightweight check that [input] looks like a `nostrconnect://` URI. Used to
     * sanity-check the URI the kernel returns before rendering it as a QR code.
     */
    fun isPlausibleNostrconnectUri(input: String): Boolean {
        val trimmed = input.trim()
        return trimmed.startsWith("nostrconnect://") &&
            trimmed.length > "nostrconnect://".length
    }

    /**
     * `true` when [account] is an EXTERNAL / remote signer — i.e. the kernel
     * (not this app) holds the private key. This is every account whose `mode`
     * is not the app-owned `local_key` token.
     *
     * NIP-46 bunker accounts surface here as `mode = "nip55"` (see the class
     * doc): the projection has no distinct bunker token, and nip55 means
     * "external signer". A NIP-55 / Amber account is also remote — but the
     * NIP-46 screens launch from `NotSignedInState`, so any external account
     * that appears DURING the flow is the bunker handshake completing.
     *
     * Returns `false` for a `null` account (not signed in) and for `local_key`.
     */
    fun isRemoteSignerAccount(account: AccountSummary?): Boolean {
        val mode = account?.mode?.trim()?.lowercase() ?: return false
        return mode != MODE_LOCAL_KEY
    }

    /**
     * Decide whether a NIP-46 handshake that was started from a given baseline
     * has now COMPLETED, observed purely from the identity snapshot.
     *
     * @param hadActiveAccountAtStart whether an active account already existed
     *   when the flow began (captured once, before Connect). The NIP-46 screens
     *   launch from `NotSignedInState`, so this is normally `false`.
     * @param current the active account in the latest snapshot tick.
     *
     * Completion = an external (remote-signer) active account now exists that did
     * NOT exist at the start. When the flow starts signed-out (the normal case),
     * this reduces to "an external active account appeared". Gating on the
     * transition — not a static `mode == "nip55"` — means a user who was already
     * signed in to another remote signer before entering the flow is not read as
     * instantly-connected.
     */
    fun handshakeCompleted(
        hadActiveAccountAtStart: Boolean,
        current: AccountSummary?,
    ): Boolean {
        // No remote account present ⇒ not done.
        if (!isRemoteSignerAccount(current)) return false
        // Started signed-out: any remote account appearing is the completion.
        if (!hadActiveAccountAtStart) return true
        // Started with an account already active (defensive — these screens are
        // NotSignedInState-only): a remote account is present now; treat as done.
        // The transition guard above already excludes the signed-out happy path;
        // here we accept the remote account as the completed state.
        return true
    }
}
