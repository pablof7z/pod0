package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.Switch
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.ProviderModelCatalogService
import io.f7z.podcast.ProviderModelOption
import io.f7z.podcast.SettingsSnapshot
import kotlinx.coroutines.launch

@Composable
fun ProviderModelSettingsScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val settings = snapshot?.settings ?: SettingsSnapshot()
    val scope = rememberCoroutineScope()
    var models by remember { mutableStateOf<List<ProviderModelOption>>(emptyList()) }
    var isLoading by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var selectedRole by remember { mutableStateOf<ProviderModelRole?>(null) }

    suspend fun loadCatalog() {
        isLoading = true
        errorMessage = null
        runCatching { ProviderModelCatalogService.fetchModels(bridge) }
            .onSuccess { models = it }
            .onFailure { errorMessage = it.message ?: "Provider catalog failed" }
        isLoading = false
    }

    LaunchedEffect(bridge) {
        loadCatalog()
    }

    LazyColumn(
        modifier = modifier
            .fillMaxSize()
            .padding(horizontal = 16.dp),
        verticalArrangement = Arrangement.spacedBy(12.dp),
        contentPadding = PaddingValues(vertical = 16.dp),
    ) {
        item {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                IconButton(onClick = onBack) {
                    Icon(Icons.AutoMirrored.Filled.ArrowBack, contentDescription = "Back")
                }
                Text(
                    text = "Providers & models",
                    style = MaterialTheme.typography.headlineSmall,
                    fontWeight = FontWeight.SemiBold,
                    modifier = Modifier.weight(1f),
                )
                IconButton(
                    onClick = { scope.launch { loadCatalog() } },
                    enabled = !isLoading,
                ) {
                    Icon(Icons.Filled.Refresh, contentDescription = "Refresh models")
                }
            }
        }

        item {
            CatalogStatusCard(
                modelCount = models.size,
                isLoading = isLoading,
                errorMessage = errorMessage,
            )
        }

        item {
            ProviderCredentialSettingsSection(settings = settings, bridge = bridge)
        }

        item {
            SpeechProviderSettingsSection(settings = settings, bridge = bridge)
        }

        item {
            Text(
                text = "LANGUAGE ROLES",
                style = MaterialTheme.typography.labelMedium,
                fontWeight = FontWeight.SemiBold,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                modifier = Modifier.padding(start = 4.dp),
            )
        }

        items(ProviderModelRole.entries, key = { it.name }) { role ->
            ProviderModelRoleRow(
                role = role,
                settings = settings,
                catalogModel = models.firstOrNull { it.id == role.modelId(settings) },
                onClick = { selectedRole = role },
            )
        }

        item {
            RerankerRow(settings = settings, bridge = bridge)
        }
    }

    val role = selectedRole
    if (role != null) {
        ProviderModelSelectorSheet(
            role = role,
            models = models,
            currentModelId = role.modelId(settings),
            currentModelName = role.modelName(settings),
            isLoading = isLoading,
            errorMessage = errorMessage,
            onRefresh = { scope.launch { loadCatalog() } },
            onDismiss = { selectedRole = null },
            onSelect = { modelId, modelName ->
                role.dispatchSelection(bridge, modelId, modelName)
                selectedRole = null
            },
        )
    }
}

@Composable
private fun CatalogStatusCard(modelCount: Int, isLoading: Boolean, errorMessage: String?) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            if (isLoading) {
                CircularProgressIndicator()
            }
            Column(modifier = Modifier.weight(1f)) {
                Text(
                    text = if (isLoading && modelCount == 0) "Loading models" else "$modelCount models",
                    style = MaterialTheme.typography.bodyLarge,
                    fontWeight = FontWeight.Medium,
                )
                val detail = errorMessage ?: "OpenRouter and Ollama"
                Text(
                    text = detail,
                    style = MaterialTheme.typography.bodySmall,
                    color = if (errorMessage == null) {
                        MaterialTheme.colorScheme.onSurfaceVariant
                    } else {
                        MaterialTheme.colorScheme.error
                    },
                    maxLines = 2,
                    overflow = TextOverflow.Ellipsis,
                )
            }
        }
    }
}

@Composable
private fun ProviderModelRoleRow(
    role: ProviderModelRole,
    settings: SettingsSnapshot,
    catalogModel: ProviderModelOption?,
    onClick: () -> Unit,
) {
    Card(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick),
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(4.dp),
        ) {
            Text(
                text = role.title,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
            )
            Text(
                text = displayModelName(role.modelId(settings), role.modelName(settings)),
                style = MaterialTheme.typography.bodyMedium,
                color = MaterialTheme.colorScheme.primary,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = catalogModel?.summaryLine ?: role.modelId(settings),
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
    }
}

@Composable
private fun RerankerRow(settings: SettingsSnapshot, bridge: KernelBridge) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Row(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalAlignment = Alignment.CenterVertically,
            horizontalArrangement = Arrangement.SpaceBetween,
        ) {
            Column(modifier = Modifier.weight(1f)) {
                Text(text = "Reranker", style = MaterialTheme.typography.bodyLarge)
                Text(
                    text = if (settings.rerankerEnabled) "Enabled" else "Disabled",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.onSurfaceVariant,
                )
            }
            Switch(
                checked = settings.rerankerEnabled,
                onCheckedChange = { enabled ->
                    PodcastActionDispatcher.dispatch(
                        bridge = bridge,
                        namespace = PodcastNamespace.SETTINGS,
                        payload = SetRerankerEnabledPayload(enabled = enabled),
                    )
                },
            )
        }
    }
}
