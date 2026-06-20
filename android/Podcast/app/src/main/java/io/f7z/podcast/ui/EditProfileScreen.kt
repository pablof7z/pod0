package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.imePadding
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material3.Button
import androidx.compose.material3.CircularProgressIndicator
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.material3.TopAppBar
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.rememberCoroutineScope
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.unit.dp
import io.f7z.podcast.DispatchResult
import io.f7z.podcast.IdentityActions
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.PodcastSnapshot
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.launch
import kotlinx.coroutines.withContext

/**
 * Edit Profile screen — Android parity for iOS `EditProfileView`.
 *
 * Fields: display name / username (name) / about / picture URL.
 * Prefill strategy (mirrors iOS `EditProfileView.hydrateFromIdentity`):
 *  - `display_name` + `picture_url` — from [PodcastSnapshot.activeAccount]
 *    ([AccountSummary]) which is already projected by the kernel.
 *  - `name` (slug) + `about` — NOT in the snapshot projection; loaded from
 *    [IdentityActions.loadCachedProfile] (Android SharedPreferences, keyed by
 *    pubkeyHex). Mirrors iOS UserDefaults `kind0CachePrefix` prefill.
 *
 * Save dispatches `podcast.social` `publish_profile` via
 * [IdentityActions.publishProfile], which calls
 * `bridge.dispatchAction("podcast.social", payload)` — the same generic seam
 * Android uses for all kernel namespaces. The kernel signs the resulting kind:0
 * event with the active account; no signing happens in Android code.
 *
 * The form stays open on failure (the error message says "Try again") and
 * dismisses automatically on success — matching iOS UX intent.
 *
 * Field limits (matching iOS [EditProfileView.Limits]):
 *  - Display name: 48 chars max
 *  - Username:     32 chars max
 *  - About:        280 chars; character counter shown when ≤ 50 remaining
 *  - Picture URL:  no hard limit (URL length)
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun EditProfileScreen(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onBack: () -> Unit,
    modifier: Modifier = Modifier,
) {
    val context = LocalContext.current
    val scope = rememberCoroutineScope()
    val account = snapshot?.activeAccount
    val pubkeyHex = account?.pubkeyHex.orEmpty()

    var displayName by remember { mutableStateOf("") }
    var name by remember { mutableStateOf("") }
    var about by remember { mutableStateOf("") }
    var pictureUrl by remember { mutableStateOf("") }

    // Track the initial state so Save is disabled until the user makes a change.
    var initialDisplayName by remember { mutableStateOf("") }
    var initialName by remember { mutableStateOf("") }
    var initialAbout by remember { mutableStateOf("") }
    var initialPictureUrl by remember { mutableStateOf("") }
    var hydrated by remember { mutableStateOf(false) }

    var isPublishing by remember { mutableStateOf(false) }
    var errorMessage by remember { mutableStateOf<String?>(null) }
    var successMessage by remember { mutableStateOf<String?>(null) }

    // Hydrate once from snapshot + local cache — mirrors hydrateFromIdentity().
    LaunchedEffect(pubkeyHex) {
        if (pubkeyHex.isEmpty()) return@LaunchedEffect
        val cached = withContext(Dispatchers.IO) {
            IdentityActions.loadCachedProfile(context, pubkeyHex)
        }
        // Projected fields come from snapshot; non-projected from local cache.
        displayName = account?.displayName.orEmpty()
        pictureUrl = account?.pictureUrl.orEmpty()
        name = cached.name
        about = cached.about
        // Record initial state for dirty detection.
        initialDisplayName = displayName
        initialPictureUrl = pictureUrl
        initialName = name
        initialAbout = about
        hydrated = true
    }

    val isDirty = hydrated && (
        displayName != initialDisplayName ||
        name != initialName ||
        about != initialAbout ||
        pictureUrl != initialPictureUrl
    )

    Scaffold(
        modifier = modifier,
        topBar = {
            TopAppBar(
                title = { Text("Edit Profile") },
                navigationIcon = {
                    IconButton(onClick = onBack) {
                        Icon(
                            imageVector = Icons.AutoMirrored.Filled.ArrowBack,
                            contentDescription = "Back",
                        )
                    }
                },
            )
        },
    ) { inner ->
        Column(
            modifier = Modifier
                .padding(inner)
                .fillMaxSize()
                .verticalScroll(rememberScrollState())
                .imePadding()
                .padding(horizontal = 16.dp, vertical = 16.dp),
            verticalArrangement = Arrangement.spacedBy(16.dp),
        ) {
            // Display name — projected by snapshot, 48-char limit.
            OutlinedTextField(
                value = displayName,
                onValueChange = { new ->
                    displayName = if (new.length > 48) new.take(48) else new
                    errorMessage = null
                },
                label = { Text("Display name") },
                placeholder = { Text("e.g. Bright Signal") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            // Username (Nostr `name` field) — not projected; cached locally. 32-char limit.
            OutlinedTextField(
                value = name,
                onValueChange = { new ->
                    name = if (new.length > 32) new.take(32) else new
                    errorMessage = null
                },
                label = { Text("Username") },
                placeholder = { Text("bright-signal-a3f2") },
                supportingText = {
                    Text(
                        "Used to sign your contributions. Letters, numbers, and dashes work best.",
                        style = MaterialTheme.typography.bodySmall,
                        color = MaterialTheme.colorScheme.onSurfaceVariant,
                    )
                },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            // About — not projected; cached locally. 280-char limit with counter.
            val aboutRemaining = 280 - about.length
            OutlinedTextField(
                value = about,
                onValueChange = { new ->
                    about = if (new.length > 280) new.take(280) else new
                    errorMessage = null
                },
                label = { Text("About") },
                placeholder = { Text("Tell people who you are.") },
                supportingText = if (aboutRemaining <= 50) {
                    { Text("$aboutRemaining characters left") }
                } else null,
                minLines = 3,
                maxLines = 6,
                modifier = Modifier.fillMaxWidth(),
            )

            // Picture URL — projected by snapshot (no char limit enforced here).
            OutlinedTextField(
                value = pictureUrl,
                onValueChange = { new ->
                    pictureUrl = new
                    errorMessage = null
                },
                label = { Text("Picture URL") },
                placeholder = { Text("https://example.com/avatar.jpg") },
                singleLine = true,
                modifier = Modifier.fillMaxWidth(),
            )

            // Inline status messages — error stays until next edit; success shown briefly.
            errorMessage?.let { msg ->
                Text(
                    text = msg,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.error,
                )
            }
            successMessage?.let { msg ->
                Text(
                    text = msg,
                    style = MaterialTheme.typography.bodySmall,
                    color = MaterialTheme.colorScheme.primary,
                    fontWeight = FontWeight.Medium,
                )
            }

            // Save button — disabled until dirty, spinner while publishing.
            Button(
                onClick = {
                    if (isPublishing || pubkeyHex.isEmpty()) return@Button
                    scope.launch {
                        isPublishing = true
                        errorMessage = null
                        successMessage = null
                        // The kernel returns {"correlation_id":"..."} on accept or
                        // {"error":"..."} on reject — parsed by DispatchResult.parseEnvelope.
                        // IdentityActions.publishProfile caches non-projected fields ONLY on
                        // Accepted, so a rejected dispatch leaves local state untouched.
                        val dispatchResult = withContext(Dispatchers.IO) {
                            IdentityActions.publishProfile(
                                bridge = bridge,
                                context = context,
                                pubkeyHex = pubkeyHex,
                                name = name,
                                displayName = displayName,
                                about = about,
                                pictureUrl = pictureUrl,
                            )
                        }
                        isPublishing = false
                        when (dispatchResult) {
                            is DispatchResult.Failure -> {
                                // Surface the kernel's rejection reason or the FFI error.
                                // Do NOT advance snapshot or dismiss — the edit is not saved.
                                errorMessage = dispatchResult.message
                            }
                            is DispatchResult.Accepted -> {
                                // Advance initial snapshot so Save button disables again.
                                initialDisplayName = displayName
                                initialName = name
                                initialAbout = about
                                initialPictureUrl = pictureUrl
                                // Kernel dispatch is fire-and-forget (enqueue envelope,
                                // not relay confirmation), so do not claim "published".
                                successMessage = "Profile update sent."
                                // Brief success beat then dismiss — matches iOS 900 ms intent.
                                kotlinx.coroutines.delay(900)
                                onBack()
                            }
                        }
                    }
                },
                enabled = isDirty && !isPublishing && pubkeyHex.isNotEmpty(),
                modifier = Modifier.fillMaxWidth(),
            ) {
                if (isPublishing) {
                    CircularProgressIndicator(
                        modifier = Modifier.padding(end = 8.dp),
                        strokeWidth = 2.dp,
                        color = MaterialTheme.colorScheme.onPrimary,
                    )
                }
                Text(if (isPublishing) "Publishing…" else "Save")
            }
        }
    }
}
