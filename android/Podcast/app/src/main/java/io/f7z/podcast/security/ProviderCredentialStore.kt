package io.f7z.podcast.security

import android.content.Context
import android.content.SharedPreferences
import androidx.security.crypto.EncryptedSharedPreferences
import androidx.security.crypto.MasterKey

/**
 * Encrypted at-rest persistence for LLM provider API keys on Android.
 *
 * The Rust kernel owns provider selection, model routing, and HTTP transport.
 * Android owns only the host secret store, then pushes the current keys into
 * Rust's in-memory provider cache through `podcast.settings`.
 */
object ProviderCredentialStore {
    private const val PREFS_FILE = "io.f7z.podcast.providers.secure"
    private const val KEY_OPEN_ROUTER = "open_router_api_key"
    private const val KEY_OLLAMA = "ollama_api_key"

    fun saveOpenRouterApiKey(context: Context, apiKey: String): Boolean =
        saveApiKey(context, KEY_OPEN_ROUTER, apiKey)

    fun loadOpenRouterApiKey(context: Context): String? =
        loadApiKey(context, KEY_OPEN_ROUTER)

    fun hasOpenRouterApiKey(context: Context): Boolean =
        loadOpenRouterApiKey(context) != null

    fun clearOpenRouterApiKey(context: Context): Boolean =
        clearApiKey(context, KEY_OPEN_ROUTER)

    fun saveOllamaApiKey(context: Context, apiKey: String): Boolean =
        saveApiKey(context, KEY_OLLAMA, apiKey)

    fun loadOllamaApiKey(context: Context): String? =
        loadApiKey(context, KEY_OLLAMA)

    fun hasOllamaApiKey(context: Context): Boolean =
        loadOllamaApiKey(context) != null

    fun clearOllamaApiKey(context: Context): Boolean =
        clearApiKey(context, KEY_OLLAMA)

    private fun saveApiKey(context: Context, keyName: String, apiKey: String): Boolean {
        val trimmed = apiKey.trim()
        if (trimmed.isEmpty()) return false
        return runCatching {
            prefs(context).edit().putString(keyName, trimmed).apply()
            true
        }.getOrElse { false }
    }

    private fun loadApiKey(context: Context, keyName: String): String? = runCatching {
        prefs(context).getString(keyName, null)?.trim()?.takeIf { it.isNotEmpty() }
    }.getOrElse {
        runCatching { context.applicationContext.deleteSharedPreferences(PREFS_FILE) }
        null
    }

    private fun clearApiKey(context: Context, keyName: String): Boolean =
        runCatching {
            prefs(context).edit().remove(keyName).apply()
            true
        }.getOrElse {
            runCatching { context.applicationContext.deleteSharedPreferences(PREFS_FILE) }
                .getOrDefault(false)
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
