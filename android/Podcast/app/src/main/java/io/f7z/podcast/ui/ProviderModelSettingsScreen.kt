package io.f7z.podcast.ui

import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.PaddingValues
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.heightIn
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.lazy.LazyColumn
import androidx.compose.foundation.lazy.items
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.Refresh
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
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
                    text = "Models",
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

@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun ProviderModelSelectorSheet(
    role: ProviderModelRole,
    models: List<ProviderModelOption>,
    currentModelId: String,
    currentModelName: String,
    isLoading: Boolean,
    errorMessage: String?,
    onRefresh: () -> Unit,
    onDismiss: () -> Unit,
    onSelect: (String, String) -> Unit,
) {
    var searchText by remember(role) { mutableStateOf("") }
    var manualModelId by remember(role, currentModelId) { mutableStateOf(currentModelId) }
    val visibleModels = models.filter { it.matches(searchText) }.take(MAX_VISIBLE_MODELS)

    ModalBottomSheet(onDismissRequest = onDismiss) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 16.dp),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Row(verticalAlignment = Alignment.CenterVertically) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = role.title,
                        style = MaterialTheme.typography.titleLarge,
                        fontWeight = FontWeight.SemiBold,
                    )
                    Text(
                        text = displayModelName(currentModelId, currentModelName),
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                        maxLines = 1,
                        overflow = TextOverflow.Ellipsis,
                    )
                }
                IconButton(onClick = onRefresh, enabled = !isLoading) {
                    Icon(Icons.Filled.Refresh, contentDescription = "Refresh models")
                }
            }

            OutlinedTextField(
                value = searchText,
                onValueChange = { searchText = it },
                label = { Text("Search models") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            if (errorMessage != null) {
                Text(
                    text = errorMessage,
                    color = MaterialTheme.colorScheme.error,
                    style = MaterialTheme.typography.bodySmall,
                    maxLines = 3,
                    overflow = TextOverflow.Ellipsis,
                )
            }

            LazyColumn(
                modifier = Modifier
                    .fillMaxWidth()
                    .heightIn(max = 520.dp),
                contentPadding = PaddingValues(bottom = 24.dp),
            ) {
                if (isLoading && models.isEmpty()) {
                    item {
                        Row(
                            modifier = Modifier
                                .fillMaxWidth()
                                .padding(vertical = 16.dp),
                            verticalAlignment = Alignment.CenterVertically,
                            horizontalArrangement = Arrangement.spacedBy(12.dp),
                        ) {
                            CircularProgressIndicator()
                            Text("Loading models")
                        }
                    }
                }

                items(visibleModels, key = { it.id }) { model ->
                    ProviderModelCatalogRow(
                        model = model,
                        isSelected = model.id == currentModelId,
                        onClick = { onSelect(model.id, model.displayName) },
                    )
                    HorizontalDivider()
                }

                if (visibleModels.isEmpty() && !isLoading) {
                    item {
                        Text(
                            text = "No models match this search",
                            style = MaterialTheme.typography.bodyMedium,
                            color = MaterialTheme.colorScheme.onSurfaceVariant,
                            modifier = Modifier.padding(vertical = 16.dp),
                        )
                    }
                }

                item {
                    Column(
                        modifier = Modifier.padding(top = 12.dp),
                        verticalArrangement = Arrangement.spacedBy(8.dp),
                    ) {
                        OutlinedTextField(
                            value = manualModelId,
                            onValueChange = { manualModelId = it },
                            label = { Text("Custom model ID") },
                            singleLine = true,
                            modifier = Modifier.fillMaxWidth(),
                        )
                        Button(
                            onClick = { onSelect(manualModelId.trim(), "") },
                            enabled = manualModelId.isNotBlank(),
                        ) {
                            Text("Use custom ID")
                        }
                    }
                }
            }
        }
    }
}

@Composable
private fun ProviderModelCatalogRow(
    model: ProviderModelOption,
    isSelected: Boolean,
    onClick: () -> Unit,
) {
    Row(
        modifier = Modifier
            .fillMaxWidth()
            .clickable(onClick = onClick)
            .padding(vertical = 12.dp),
        verticalAlignment = Alignment.Top,
    ) {
        Column(modifier = Modifier.weight(1f), verticalArrangement = Arrangement.spacedBy(3.dp)) {
            Text(
                text = model.displayName,
                style = MaterialTheme.typography.bodyLarge,
                fontWeight = FontWeight.Medium,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = model.id,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 1,
                overflow = TextOverflow.Ellipsis,
            )
            Text(
                text = model.summaryLine,
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
                maxLines = 2,
                overflow = TextOverflow.Ellipsis,
            )
        }
        Spacer(modifier = Modifier.width(12.dp))
        Text(
            text = if (isSelected) "Selected" else model.compactPricing,
            style = MaterialTheme.typography.labelMedium,
            color = if (isSelected) MaterialTheme.colorScheme.primary else MaterialTheme.colorScheme.onSurfaceVariant,
        )
    }
}

private val ProviderModelOption.summaryLine: String
    get() = listOfNotNull(
        providerName.ifBlank { providerId },
        contextLabel,
        compactPricing,
        if (supportsTools) "Tools" else null,
        if (supportsReasoning) "Reasoning" else null,
        if (!isCompatible) "No JSON" else null,
    ).joinToString(" / ")

private fun displayModelName(modelId: String, modelName: String): String {
    if (modelName.isNotBlank()) return modelName
    return modelId.substringAfter("ollama:").substringAfterLast('/').ifBlank { modelId }
}

private const val MAX_VISIBLE_MODELS = 200
