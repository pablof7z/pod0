package io.f7z.podcast.security

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Encrypted at-rest persistence for the user's Nostr private key (`nsec`).
 *
 * Backed by [EncryptedSharedPreferences], whose values are AES-256-GCM
 * encrypted under a key held in the hardware-backed Android Keystore. The
 * `nsec` never appears in plaintext on disk — only the Keystore-wrapped
 * ciphertext does — which is what lets the Identity surface claim "your
 * private key never leaves this device".
 *
 * **Why the nsec also lives here, not only in the kernel:** the Rust kernel's
 * `IdentityStore::import_nsec` now persists `identity.json` on Android too —
 * `KernelBridge.setDataDir` (wired in `MainActivity` before `start`) binds the
 * kernel `data_dir`, so kernel-side state survives process restarts the same
 * way it does on iOS, and the launch-time kernel reload is what restores the
 * signed-in state into the first snapshot. This manager remains the right home
 * for the raw `nsec`: it is a hardware-Keystore-backed secret store, whereas the
 * kernel `identity.json` is a plaintext shadow. `MainActivity` re-imports the
 * stored nsec into the kernel on launch; that import is idempotent and the
 * secret never leaves this device in plaintext.
 *
 * All methods tolerate Keystore/crypto failures (corrupted prefs, key-rotation
 * edge cases): a failed read falls back to `null` and self-heals by clearing
 * the offending store so the next sign-in can write fresh. Callers treat a
 * `null` load as "not signed in".
 */
object KeystoreManager {
    private const val PREFS_FILE = "io.f7z.podcast.identity.secure"
    private const val KEY_NSEC = "nostr_nsec"

    /** Encrypt and persist [nsec] under the secure store. Overwrites any prior value. */
    fun saveNsec(context: Context, nsec: String) {
        runCatching { prefs(context).edit().putString(KEY_NSEC, nsec).apply() }
    }

    /**
     * Load the stored nsec, or `null` when none is stored (or the secure
     * store could not be opened/decrypted). A decrypt failure clears the
     * store so a subsequent sign-in starts clean.
     */
    fun loadNsec(context: Context): String? = runCatching {
        prefs(context).getString(KEY_NSEC, null)
    }.getOrElse {
        // Decryption/keystore failure — drop the unreadable store so we don't
        // wedge sign-in forever, and report "not signed in".
        runCatching { context.applicationContext.deleteSharedPreferences(PREFS_FILE) }
        null
    }

    /** Remove the stored nsec. Idempotent. Silently no-ops on Keystore failure. */
    fun clearNsec(context: Context) {
        runCatching { prefs(context).edit().remove(KEY_NSEC).apply() }.getOrElse {
            // If we can't open the store, delete the whole file so the nsec
            // cannot auto-restore on next launch.
            runCatching { context.applicationContext.deleteSharedPreferences(PREFS_FILE) }
        }
    }

    private fun prefs(context: Context): SharedPreferences {
        val appContext = context.applicationContext
        val masterKey = MasterKey.Builder(appContext)
            .setKeyScheme(MasterKey.KeyScheme.AES256_GCM)
            .build()
        return EncryptedSharedPreferences.create(
            appContext,
            PREFS_FILE,
            masterKey,
            EncryptedSharedPreferences.PrefKeyEncryptionScheme.AES256_SIV,
            EncryptedSharedPreferences.PrefValueEncryptionScheme.AES256_GCM,
        )
    }
}
