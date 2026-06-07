package io.f7z.podcast

import android.content.Context
import io.f7z.podcast.security.ProviderCredentialStore
import io.f7z.podcast.ui.PodcastActionDispatcher
import io.f7z.podcast.ui.PodcastNamespace
import io.f7z.podcast.ui.SetOllamaCredentialPayload
import io.f7z.podcast.ui.SetOpenRouterCredentialPayload
import io.f7z.podcast.ui.SetProviderApiKeysPayload

data class ProviderCredentialActionResult(
    val ok: Boolean,
    val message: String,
)

/**
 * Host-owned provider secret bridge.
 *
 * Rust owns provider settings, selected models, and network transport; Android
 * owns secure secret persistence and reloads the current keys into Rust's
 * in-memory provider cache on launch and after every save/delete.
 */
object ProviderCredentialActions {
    fun reloadProviderApiKeys(context: Context, bridge: KernelBridge): String? =
        PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.SETTINGS,
            payload = SetProviderApiKeysPayload(
                openRouter = ProviderCredentialStore.loadOpenRouterApiKey(context),
                ollama = ProviderCredentialStore.loadOllamaApiKey(context),
            ),
        )

    fun saveOpenRouterManual(
        context: Context,
        bridge: KernelBridge,
        apiKey: String,
    ): ProviderCredentialActionResult {
        if (!ProviderCredentialStore.saveOpenRouterApiKey(context, apiKey)) {
            return ProviderCredentialActionResult(false, "OpenRouter key could not be saved.")
        }
        val metadata = PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.SETTINGS,
            payload = SetOpenRouterCredentialPayload(
                source = SOURCE_MANUAL,
                connectedAt = epochSeconds(),
            ),
        )
        val reload = reloadProviderApiKeys(context, bridge)
        return if (metadata != null && reload != null) {
            ProviderCredentialActionResult(true, "OpenRouter connected.")
        } else {
            ProviderCredentialActionResult(false, "OpenRouter key saved, but provider state did not update.")
        }
    }

    fun clearOpenRouter(
        context: Context,
        bridge: KernelBridge,
    ): ProviderCredentialActionResult {
        if (!ProviderCredentialStore.clearOpenRouterApiKey(context)) {
            return ProviderCredentialActionResult(false, "OpenRouter key could not be deleted.")
        }
        val metadata = PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.SETTINGS,
            payload = SetOpenRouterCredentialPayload(source = SOURCE_NONE),
        )
        val reload = reloadProviderApiKeys(context, bridge)
        return if (metadata != null && reload != null) {
            ProviderCredentialActionResult(true, "OpenRouter disconnected.")
        } else {
            ProviderCredentialActionResult(false, "OpenRouter key deleted, but provider state did not update.")
        }
    }

    fun saveOllamaManual(
        context: Context,
        bridge: KernelBridge,
        apiKey: String,
    ): ProviderCredentialActionResult {
        if (!ProviderCredentialStore.saveOllamaApiKey(context, apiKey)) {
            return ProviderCredentialActionResult(false, "Ollama key could not be saved.")
        }
        val metadata = PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.SETTINGS,
            payload = SetOllamaCredentialPayload(
                source = SOURCE_MANUAL,
                connectedAt = epochSeconds(),
            ),
        )
        val reload = reloadProviderApiKeys(context, bridge)
        return if (metadata != null && reload != null) {
            ProviderCredentialActionResult(true, "Ollama connected.")
        } else {
            ProviderCredentialActionResult(false, "Ollama key saved, but provider state did not update.")
        }
    }

    fun clearOllama(
        context: Context,
        bridge: KernelBridge,
    ): ProviderCredentialActionResult {
        if (!ProviderCredentialStore.clearOllamaApiKey(context)) {
            return ProviderCredentialActionResult(false, "Ollama key could not be deleted.")
        }
        val metadata = PodcastActionDispatcher.dispatch(
            bridge = bridge,
            namespace = PodcastNamespace.SETTINGS,
            payload = SetOllamaCredentialPayload(source = SOURCE_NONE),
        )
        val reload = reloadProviderApiKeys(context, bridge)
        return if (metadata != null && reload != null) {
            ProviderCredentialActionResult(true, "Ollama disconnected.")
        } else {
            ProviderCredentialActionResult(false, "Ollama key deleted, but provider state did not update.")
        }
    }

    private fun epochSeconds(): Long = System.currentTimeMillis() / 1000L

    private const val SOURCE_NONE = "none"
    private const val SOURCE_MANUAL = "manual"
}
