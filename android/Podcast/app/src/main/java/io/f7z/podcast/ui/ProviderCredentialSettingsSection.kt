package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.ElevenLabsKeyValidationService
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.OpenRouterKeyValidationService
import io.f7z.podcast.ProviderCredentialActions
import io.f7z.podcast.ProviderCredentialActionResult
import io.f7z.podcast.SettingsSnapshot
import io.f7z.podcast.security.ProviderCredentialStore
import kotlinx.coroutines.launch

@Composable
fun ProviderCredentialSettingsSection(
    settings: SettingsSnapshot,
    bridge: KernelBridge,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    var hasOpenRouterKey by remember { mutableStateOf(ProviderCredentialStore.hasOpenRouterApiKey(context)) }
    var hasOllamaKey by remember { mutableStateOf(ProviderCredentialStore.hasOllamaApiKey(context)) }
    var hasElevenLabsKey by remember { mutableStateOf(ProviderCredentialStore.hasElevenLabsApiKey(context)) }
    var hasAssemblyAiKey by remember { mutableStateOf(ProviderCredentialStore.hasAssemblyAiApiKey(context)) }
    var openRouterInput by remember { mutableStateOf("") }
    var ollamaInput by remember { mutableStateOf("") }
    var elevenLabsInput by remember { mutableStateOf("") }
    var assemblyAiInput by remember { mutableStateOf("") }
    var ollamaUrlInput by remember(settings.ollamaChatUrl) {
        mutableStateOf(settings.ollamaChatUrl.ifBlank { DEFAULT_OLLAMA_CHAT_URL })
    }
    var openRouterResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var openRouterValidationResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var isValidatingOpenRouter by remember { mutableStateOf(false) }
    var ollamaResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var ollamaUrlResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var elevenLabsResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var elevenLabsValidationResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }
    var isValidatingElevenLabs by remember { mutableStateOf(false) }
    var assemblyAiResult by remember { mutableStateOf<ProviderCredentialActionResult?>(null) }

    Column(
        modifier = modifier.fillMaxWidth(),
        verticalArrangement = Arrangement.spacedBy(8.dp),
    ) {
        Text(
            text = "PROVIDER CREDENTIALS",
            style = MaterialTheme.typography.labelMedium,
            fontWeight = FontWeight.SemiBold,
            color = MaterialTheme.colorScheme.onSurfaceVariant,
            modifier = Modifier.padding(start = 4.dp),
        )
        OpenRouterCredentialCard(
            input = openRouterInput,
            hasStoredKey = hasOpenRouterKey,
            status = credentialStatus(
                source = settings.openRouterCredentialSource,
                hasStoredKey = hasOpenRouterKey,
                keyLabel = settings.openRouterByokKeyLabel,
            ),
            result = openRouterResult,
            onInputChanged = {
                openRouterInput = it
                openRouterResult = null
            },
            onSave = {
                val result = ProviderCredentialActions.saveOpenRouterManual(context, bridge, openRouterInput)
                openRouterResult = result
                openRouterValidationResult = null
                hasOpenRouterKey = ProviderCredentialStore.hasOpenRouterApiKey(context)
                if (result.ok) openRouterInput = ""
            },
            onDisconnect = {
                val result = ProviderCredentialActions.clearOpenRouter(context, bridge)
                openRouterResult = result
                openRouterValidationResult = null
                hasOpenRouterKey = ProviderCredentialStore.hasOpenRouterApiKey(context)
                if (result.ok) openRouterInput = ""
            },
            validationResult = openRouterValidationResult,
            isValidating = isValidatingOpenRouter,
            onValidate = {
                scope.launch {
                    isValidatingOpenRouter = true
                    openRouterValidationResult = null
                    ProviderCredentialActions.reloadProviderApiKeys(context, bridge)
                    openRouterValidationResult = runCatching {
                        OpenRouterKeyValidationService.validateStoredKey(bridge)
                    }.fold(
                        onSuccess = { ProviderCredentialActionResult(true, it.summary) },
                        onFailure = {
                            ProviderCredentialActionResult(
                                false,
                                it.message ?: "OpenRouter key could not be validated.",
                            )
                        },
                    )
                    isValidatingOpenRouter = false
                }
            },
        )
        OllamaCredentialCard(
            input = ollamaInput,
            hasStoredKey = hasOllamaKey,
            status = credentialStatus(
                source = settings.ollamaCredentialSource,
                hasStoredKey = hasOllamaKey,
                keyLabel = settings.ollamaByokKeyLabel,
            ),
            urlInput = ollamaUrlInput,
            result = ollamaResult,
            urlResult = ollamaUrlResult,
            onInputChanged = {
                ollamaInput = it
                ollamaResult = null
            },
            onUrlChanged = {
                ollamaUrlInput = it
                ollamaUrlResult = null
            },
            onSave = {
                val result = ProviderCredentialActions.saveOllamaManual(context, bridge, ollamaInput)
                ollamaResult = result
                hasOllamaKey = ProviderCredentialStore.hasOllamaApiKey(context)
                if (result.ok) ollamaInput = ""
            },
            onDisconnect = {
                val result = ProviderCredentialActions.clearOllama(context, bridge)
                ollamaResult = result
                hasOllamaKey = ProviderCredentialStore.hasOllamaApiKey(context)
                if (result.ok) ollamaInput = ""
            },
            onSaveUrl = {
                val trimmed = ollamaUrlInput.trim()
                val response = PodcastActionDispatcher.dispatch(
                    bridge = bridge,
                    namespace = PodcastNamespace.SETTINGS,
                    payload = SetOllamaChatUrlPayload(url = trimmed),
                )
                ollamaUrlResult = if (response != null) {
                    ProviderCredentialActionResult(true, "Ollama endpoint saved.")
                } else {
                    ProviderCredentialActionResult(false, "Ollama endpoint could not be saved.")
                }
            },
        )
        ElevenLabsCredentialCard(
            input = elevenLabsInput,
            hasStoredKey = hasElevenLabsKey,
            status = credentialStatus(
                source = settings.elevenLabsCredentialSource,
                hasStoredKey = hasElevenLabsKey,
                keyLabel = settings.elevenLabsByokKeyLabel,
            ),
            result = elevenLabsResult,
            onInputChanged = {
                elevenLabsInput = it
                elevenLabsResult = null
                elevenLabsValidationResult = null
            },
            onSave = {
                val result = ProviderCredentialActions.saveElevenLabsManual(context, bridge, elevenLabsInput)
                elevenLabsResult = result
                elevenLabsValidationResult = null
                hasElevenLabsKey = ProviderCredentialStore.hasElevenLabsApiKey(context)
                if (result.ok) elevenLabsInput = ""
            },
            onDisconnect = {
                val result = ProviderCredentialActions.clearElevenLabs(context, bridge)
                elevenLabsResult = result
                elevenLabsValidationResult = null
                hasElevenLabsKey = ProviderCredentialStore.hasElevenLabsApiKey(context)
                if (result.ok) elevenLabsInput = ""
            },
            validationResult = elevenLabsValidationResult,
            isValidating = isValidatingElevenLabs,
            onValidate = {
                scope.launch {
                    isValidatingElevenLabs = true
                    elevenLabsValidationResult = null
                    ProviderCredentialActions.reloadProviderApiKeys(context, bridge)
                    elevenLabsValidationResult = runCatching {
                        ElevenLabsKeyValidationService.validateStoredKey(bridge)
                    }.fold(
                        onSuccess = { ProviderCredentialActionResult(true, it.summary) },
                        onFailure = {
                            ProviderCredentialActionResult(
                                false,
                                it.message ?: "ElevenLabs key could not be validated.",
                            )
                        },
                    )
                    isValidatingElevenLabs = false
                }
            },
        )
        AssemblyAiCredentialCard(
            input = assemblyAiInput,
            hasStoredKey = hasAssemblyAiKey,
            result = assemblyAiResult,
            onInputChanged = {
                assemblyAiInput = it
                assemblyAiResult = null
            },
            onSave = {
                val result = ProviderCredentialActions.saveAssemblyAiManual(context, bridge, assemblyAiInput)
                assemblyAiResult = result
                hasAssemblyAiKey = ProviderCredentialStore.hasAssemblyAiApiKey(context)
                if (result.ok) assemblyAiInput = ""
            },
            onDisconnect = {
                val result = ProviderCredentialActions.clearAssemblyAi(context, bridge)
                assemblyAiResult = result
                hasAssemblyAiKey = ProviderCredentialStore.hasAssemblyAiApiKey(context)
                if (result.ok) assemblyAiInput = ""
            },
        )
    }
}

private const val DEFAULT_OLLAMA_CHAT_URL = "https://ollama.com/api/chat"
