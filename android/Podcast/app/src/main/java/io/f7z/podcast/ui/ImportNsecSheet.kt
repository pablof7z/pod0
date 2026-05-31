package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.navigationBarsPadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material3.Button
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.ModalBottomSheet
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Text
import androidx.compose.material3.rememberModalBottomSheetState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.KeyboardType
import androidx.compose.ui.text.input.VisualTransformation
import androidx.compose.ui.unit.dp
import io.f7z.podcast.IdentityActions

/**
 * Bottom sheet for importing a local Nostr private key (`nsec`).
 *
 * Validation is intentionally light (`nsec1` prefix + length) — the kernel's
 * `Keys::parse` is the authoritative check. On submit we hand the key to
 * [onSubmit], which dispatches `podcast.identity` `ImportNsec` and persists to
 * the Android Keystore; the caller owns dismissal so the parent controls
 * sheet visibility from a single `signedIn`-derived source of truth.
 *
 * The field uses a password keyboard ([KeyboardType.Password]) so the key
 * does not land in keyboard suggestion/learning, but [VisualTransformation.None]
 * keeps the characters visible — an nsec is long and error-prone to type
 * blind, and the screen already warns the key stays on-device.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun ImportNsecSheet(
    onDismiss: () -> Unit,
    onSubmit: (String) -> Unit,
) {
    val sheetState = rememberModalBottomSheetState(skipPartiallyExpanded = true)
    var nsec by remember { mutableStateOf("") }
    var showError by remember { mutableStateOf(false) }

    ModalBottomSheet(
        onDismissRequest = onDismiss,
        sheetState = sheetState,
    ) {
        Column(
            modifier = Modifier
                .fillMaxWidth()
                .padding(horizontal = 24.dp)
                .padding(bottom = 24.dp)
                .navigationBarsPadding()
                .imePadding(),
            verticalArrangement = Arrangement.spacedBy(12.dp),
        ) {
            Text(
                text = "Import nsec key",
                style = MaterialTheme.typography.titleLarge,
                fontWeight = FontWeight.SemiBold,
            )
            OutlinedTextField(
                value = nsec,
                onValueChange = {
                    nsec = it
                    if (showError) showError = false
                },
                label = { Text("nsec1…") },
                singleLine = true,
                isError = showError,
                visualTransformation = VisualTransformation.None,
                keyboardOptions = KeyboardOptions(keyboardType = KeyboardType.Password),
                modifier = Modifier.fillMaxWidth(),
            )
            if (showError) {
                Text(
                    text = "That doesn't look like an nsec key. It should start with \"nsec1\".",
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            Text(
                text = "Your private key never leaves this device.",
                style = MaterialTheme.typography.bodySmall,
                color = MaterialTheme.colorScheme.onSurfaceVariant,
            )
            Button(
                onClick = {
                    val trimmed = nsec.trim()
                    if (IdentityActions.isPlausibleNsec(trimmed)) {
                        onSubmit(trimmed)
                    } else {
                        showError = true
                    }
                },
                modifier = Modifier.fillMaxWidth(),
            ) { Text("Sign in") }
        }
    }
}
