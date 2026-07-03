package io.f7z.podcast

import kotlinx.serialization.json.JsonElement
import kotlinx.serialization.json.JsonObject
import kotlinx.serialization.json.JsonPrimitive
import kotlinx.serialization.json.Json

/**
 * Canonical wire contract for the `podcast.identity` kernel action namespace.
 *
 * **Why not `bridge.signinNsec`?** The generated `bridge.signinNsec`
 * path calls the kernel's generic local-signer API, which feeds the *nmp-core*
 * multi-account store. The Android snapshot's `activeAccount` is built
 * elsewhere — from the podcast-app `IdentityStore` (`ffi/snapshot.rs`),
 * populated ONLY by the `podcast.identity` actions below. So sign-in/out that
 * the Identity screen can observe MUST go through `dispatchAction`, not the
 * legacy stub. The verified Rust contract (`identity_handler.rs`):
 *
 *  * `{"type":"ImportNsec","nsec":"nsec1…"}` — parse + persist + populate
 *    `active_account`.
 *  * `{"type":"Clear"}` — wipe the identity so `active_account` becomes null.
 *
 * Sign-out is `Clear`, NOT `ImportNsec` with an empty string — an empty nsec
 * hits `Keys::parse("")` → error → no-op (the identity would stick).
 */
object IdentityActions {
    const val NAMESPACE = "podcast.identity"

    /**
     * Social publish namespace for kind:0/1/9802 events.
     * Wire contract: `{"op":"publish_profile","name":"...","display_name":"...",...}`.
     * Mirrors `UserIdentityStore+Publishing.swift` dispatch seam.
     */
    const val SOCIAL_NAMESPACE = "podcast.social"

    private val json = Json

    /** Dispatch `ImportNsec` for [nsec]. Returns the kernel JSON envelope or null on FFI failure. */
    fun importNsec(bridge: KernelBridge, nsec: String): String? =
        bridge.dispatchAction(NAMESPACE, importNsecPayload(nsec))

    /** Dispatch `Generate` to create a fresh keypair. The kernel writes the new key
     *  to `identity.json` in the data dir — no Keystore entry needed (Keystore is
     *  only for imported nsec keys that the user may want to back up). */
    fun generate(bridge: KernelBridge): String? =
        bridge.dispatchAction(NAMESPACE, GENERATE_PAYLOAD)

    /** Dispatch `Clear` to sign out. Returns the kernel JSON envelope or null on FFI failure. */
    fun clear(bridge: KernelBridge): String? =
        bridge.dispatchAction(NAMESPACE, CLEAR_PAYLOAD)

    /**
     * Dispatch `publish_profile` to the `podcast.social` kernel namespace.
     *
     * Wire contract (verified against `ffi/actions/social_module.rs` `SocialAction::PublishProfile`):
     * ```json
     * {"op":"publish_profile","name":"slug","display_name":"Display","about":"…","picture":"https://…"}
     * ```
     * Field semantics (mirroring `UserIdentityStore+Publishing.swift`):
     *  - `name`         — required; the Nostr username / slug.
     *  - `display_name` — optional; omitted from JSON when blank (kernel skips absent fields).
     *  - `about`        — optional; omitted when blank.
     *  - `picture`      — optional; omitted when blank.
     *
     * The kernel signs the resulting kind:0 event with the active account — no
     * signing in Android code. Mirrors the iOS `dispatchToKernel("podcast.social",
     * body:["op":"publish_profile",…])` call exactly.
     *
     * After dispatching, the kernel self-applies the accepted profile fields to
     * [AccountSummary]. Android keeps no SharedPreferences mirror.
     *
     * Returns the kernel JSON envelope or null on FFI failure.
     */
    /**
     * Returns [DispatchResult.Accepted] when the kernel enqueued the action, or
     * [DispatchResult.Failure] on synchronous rejection or FFI failure.
     *
     * A rejected dispatch must not mutate local state; callers keep their form
     * draft open and wait for the next kernel projection after acceptance.
     */
    fun publishProfile(
        bridge: KernelBridge,
        name: String,
        displayName: String,
        about: String,
        pictureUrl: String,
    ): DispatchResult {
        val payload = buildPublishProfilePayload(name, displayName, about, pictureUrl)
        val raw = bridge.dispatchAction(SOCIAL_NAMESPACE, payload)
        return DispatchResult.parseEnvelope(raw)
    }

    /**
     * Build the `publish_profile` JSON payload.
     *
     * Extracted as a pure function so unit tests can assert the wire shape
     * without a kernel bridge. Blank optional fields are omitted (the Rust
     * `SocialAction::PublishProfile` uses `#[serde(default,
     * skip_serializing_if = "Option::is_none")]`; sending an empty string
     * would write an empty string to the kind:0 content, which is wrong).
     */
    fun buildPublishProfilePayload(
        name: String,
        displayName: String,
        about: String,
        pictureUrl: String,
    ): String {
        val fields = mutableMapOf<String, JsonElement>(
            "op" to JsonPrimitive("publish_profile"),
            "name" to JsonPrimitive(name.trim()),
        )
        val trimmedDisplayName = displayName.trim()
        if (trimmedDisplayName.isNotEmpty()) {
            fields["display_name"] = JsonPrimitive(trimmedDisplayName)
        }
        val trimmedAbout = about.trim()
        if (trimmedAbout.isNotEmpty()) {
            fields["about"] = JsonPrimitive(trimmedAbout)
        }
        val trimmedPicture = pictureUrl.trim()
        if (trimmedPicture.isNotEmpty()) {
            fields["picture"] = JsonPrimitive(trimmedPicture)
        }
        return json.encodeToString(JsonObject.serializer(), JsonObject(fields))
    }

    /** Build the `ImportNsec` payload with the nsec safely JSON-escaped. */
    fun importNsecPayload(nsec: String): String =
        json.encodeToString(
            JsonObject.serializer(),
            JsonObject(
                mapOf(
                    "type" to JsonPrimitive("ImportNsec"),
                    "nsec" to JsonPrimitive(nsec),
                ),
            ),
        )

    /**
     * Lightweight client-side format gate before we hand the key to the kernel.
     * The kernel's `Keys::parse` is the authoritative validator (and rejects on
     * the next snapshot via an unchanged `activeAccount`); this only catches the
     * obvious paste mistakes so the user gets immediate feedback.
     */
    fun isPlausibleNsec(input: String): Boolean {
        val trimmed = input.trim()
        return trimmed.startsWith("nsec1") && trimmed.length > 10
    }

    private const val CLEAR_PAYLOAD = "{\"type\":\"Clear\"}"
    private const val GENERATE_PAYLOAD = "{\"type\":\"Generate\"}"
}
