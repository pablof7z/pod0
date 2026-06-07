package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.Card
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.PasswordVisualTransformation
import androidx.compose.ui.unit.dp
import io.f7z.podcast.ProviderCredentialActionResult

@Composable
fun OpenRouterCredentialCard(
    input: String,
    hasStoredKey: Boolean,
    status: String,
    result: ProviderCredentialActionResult?,
    onInputChanged: (String) -> Unit,
    onSave: () -> Unit,
    onDisconnect: () -> Unit,
    validationResult: ProviderCredentialActionResult?,
    isValidating: Boolean,
    onValidate: () -> Unit,
) {
    CredentialCard(
        title = "OpenRouter",
        status = status,
        input = input,
        inputLabel = "OpenRouter API key",
        hasStoredKey = hasStoredKey,
        result = result,
        onInputChanged = onInputChanged,
        onSave = onSave,
        onDisconnect = onDisconnect,
    ) {
        if (hasStoredKey) {
            HorizontalDivider()
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                OutlinedButton(onClick = onValidate, enabled = !isValidating) {
                    Text(if (isValidating) "Validating" else "Validate key")
                }
            }
            ResultText(validationResult)
        }
    }
}

@Composable
fun OllamaCredentialCard(
    input: String,
    hasStoredKey: Boolean,
    status: String,
    urlInput: String,
    result: ProviderCredentialActionResult?,
    urlResult: ProviderCredentialActionResult?,
    onInputChanged: (String) -> Unit,
    onUrlChanged: (String) -> Unit,
    onSave: () -> Unit,
    onDisconnect: () -> Unit,
    onSaveUrl: () -> Unit,
) {
    CredentialCard(
        title = "Ollama",
        status = status,
        input = input,
        inputLabel = "Ollama API key",
        hasStoredKey = hasStoredKey,
        result = result,
        onInputChanged = onInputChanged,
        onSave = onSave,
        onDisconnect = onDisconnect,
    ) {
        HorizontalDivider()
        OutlinedTextField(
            value = urlInput,
            onValueChange = onUrlChanged,
            label = { Text("Chat endpoint") },
            singleLine = true,
            modifier = Modifier.fillMaxWidth(),
        )
        Row(
            modifier = Modifier.fillMaxWidth(),
            horizontalArrangement = Arrangement.End,
        ) {
            Button(onClick = onSaveUrl, enabled = urlInput.isNotBlank()) {
                Text("Save endpoint")
            }
        }
        ResultText(urlResult)
    }
}

private fun credentialButtonLabel(hasStoredKey: Boolean): String =
    if (hasStoredKey) "Replace key" else "Save key"

@Composable
private fun CredentialCard(
    title: String,
    status: String,
    input: String,
    inputLabel: String,
    hasStoredKey: Boolean,
    result: ProviderCredentialActionResult?,
    onInputChanged: (String) -> Unit,
    onSave: () -> Unit,
    onDisconnect: () -> Unit,
    extraContent: @Composable () -> Unit = {},
) {
    Card(modifier = Modifier.fillMaxWidth()) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(16.dp),
            verticalArrangement = Arrangement.spacedBy(10.dp),
        ) {
            Row(
                modifier = Modifier.fillMaxWidth(),
                verticalAlignment = Alignment.CenterVertically,
            ) {
                Column(modifier = Modifier.weight(1f)) {
                    Text(
                        text = title,
                        style = MaterialTheme.typography.bodyLarge,
                        fontWeight = FontWeight.Medium,
                    )
                    Text(
                        text = status,
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                }
                if (hasStoredKey) {
                    OutlinedButton(onClick = onDisconnect) {
                        Text("Disconnect")
                    }
                }
            }
            OutlinedTextField(
                value = input,
                onValueChange = onInputChanged,
                label = { Text(inputLabel) },
                singleLine = true,
                visualTransformation = PasswordVisualTransformation(),
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                modifier = Modifier.fillMaxWidth(),
            )
            Row(
                modifier = Modifier.fillMaxWidth(),
                horizontalArrangement = Arrangement.End,
            ) {
                Button(onClick = onSave, enabled = input.isNotBlank()) {
                    Text(credentialButtonLabel(hasStoredKey))
                }
            }
            ResultText(result)
            extraContent()
        }
    }
}

@Composable
private fun ResultText(result: ProviderCredentialActionResult?) {
    if (result == null) return
    Text(
        text = result.message,
        style = MaterialTheme.typography.bodySmall,
        color = if (result.ok) {
            MaterialTheme.colorScheme.onSurfaceVariant
        } else {
            MaterialTheme.colorScheme.error
        },
    )
}

fun credentialStatus(source: String, hasStoredKey: Boolean, keyLabel: String?): String {
    val label = keyLabel?.takeIf { it.isNotBlank() }
    return when {
        hasStoredKey && label != null -> "Connected: $label"
        hasStoredKey && source == "byok" -> "Connected with BYOK"
        hasStoredKey -> "Connected"
        source.isBlank() || source == "none" -> "Not connected"
        else -> "Reconnect required"
    }
}
