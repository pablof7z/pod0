package io.f7z.podcast.ui

import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.ui.Alignment
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.LibraryBooks
import androidx.compose.material.icons.filled.AccountBox
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Home
import androidx.compose.material.icons.filled.Inbox
import androidx.compose.material.icons.filled.PlayCircle
import androidx.compose.material.icons.filled.Search
import androidx.compose.material.icons.filled.Settings
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.Icon
import androidx.compose.material3.NavigationBar
import androidx.compose.material3.NavigationBarItem
import androidx.compose.material3.Scaffold
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.saveable.rememberSaveable
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.graphics.vector.ImageVector
import io.f7z.podcast.KernelBridge
import io.f7z.podcast.NostrConversationDto
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary

/**
 * Bottom-tab navigator wiring Home / Library / Player / Settings plus an
 * inline stack for nested surfaces reached from a tab.
 *
 * Why hand-rolled instead of `androidx.navigation.compose`?
 *
 *  * The M13.C+D scope is bottom tabs plus a small set of pushable surfaces. A nav-compose
 *    graph for that is more boilerplate (NavHost + routes + Bundle args)
 *    than the equivalent sealed-class switch below — and the dependency
 *    isn't in `build.gradle.kts` today (M2.F kept the dep list minimal).
 *  * Every screen takes a `(PodcastSnapshot?, KernelBridge)` pair. With a
 *    NavHost we'd thread those through a shared `viewModel()` or composition
 *    locals; the sealed-class form passes them directly.
 *  * The full navigator is M14 scope (deep links, back-stack restoration,
 *    process-death save state) — pulling the dep in now would be premature.
 *
 * Two state slots:
 *
 *  * `selectedTab` — which bottom tab is active. Saved across config
 *    changes via `rememberSaveable`.
 *  * `route` — sealed-class for the current inner surface. Tabs reset it
 *    to their root; nested rows push to show, episode, identity, or model
 *    settings surfaces.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppNavigation(
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onSignInWithAmber: (() -> Unit)? = null,
    onSnapshotPull: suspend () -> Unit = {},
) {
    var selectedTab by rememberSaveable { mutableStateOf(BottomTab.Home) }
    var route by rememberSaveable(stateSaver = AppRoute.Saver) { mutableStateOf<AppRoute>(AppRoute.Tab(BottomTab.Home)) }

    // Conversations nav: hold the selected conversation so the detail screen
    // can find it by root-event-id even if the snapshot ticks between taps.
    var selectedConversationId by rememberSaveable { mutableStateOf<String?>(null) }

    val onTabSelected: (BottomTab) -> Unit = { tab ->
        selectedTab = tab
        route = AppRoute.Tab(tab)
    }

    val onOpenClips: () -> Unit = { route = AppRoute.ClipList }
    val onOpenBookmarks: () -> Unit = { route = AppRoute.Bookmarks }
    val onOpenFollowing: () -> Unit = { route = AppRoute.Following }
    val onOpenScheduledTasks: () -> Unit = { route = AppRoute.ScheduledTasks }

    Scaffold(
        bottomBar = {
            NavigationBar {
                BottomTab.entries.forEach { tab ->
                    NavigationBarItem(
                        selected = selectedTab == tab,
                        onClick = { onTabSelected(tab) },
                        icon = { Icon(imageVector = tab.icon, contentDescription = tab.label) },
                        label = { Text(tab.label) },
                    )
                }
            }
        },
    ) { inner ->
        val contentModifier = Modifier.fillMaxSize().padding(inner)
        when (val current = route) {
            is AppRoute.Tab -> TabContent(
                tab = current.tab,
                snapshot = snapshot,
                bridge = bridge,
                onShowSelected = { route = AppRoute.ShowDetail(it.id) },
                onOpenIdentity = { route = AppRoute.Identity },
                onOpenModels = { route = AppRoute.ProviderModels },
                onOpenNostrConversations = { route = AppRoute.NostrConversations },
                onOpenClips = onOpenClips,
                onOpenBookmarks = onOpenBookmarks,
                onOpenFollowing = onOpenFollowing,
                onOpenScheduledTasks = onOpenScheduledTasks,
                modifier = contentModifier,
            )
            is AppRoute.ShowDetail -> ShowDetailScreen(
                showId = current.showId,
                snapshot = snapshot,
                bridge = bridge,
                onEpisodeSelected = { episode ->
                    route = AppRoute.EpisodeDetail(
                        episodeId = episode.id,
                        podcastId = episode.podcastId ?: current.showId,
                    )
                },
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
            is AppRoute.EpisodeDetail -> EpisodeDetailScreen(
                episodeId = current.episodeId,
                podcastId = current.podcastId,
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.ShowDetail(current.podcastId) },
                modifier = contentModifier,
            )
            AppRoute.Identity -> IdentityScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                onSignInWithAmber = onSignInWithAmber,
                onSnapshotPull = onSnapshotPull,
                onEditProfile = { route = AppRoute.EditProfile },
                onOpenRemoteSigner = { route = AppRoute.RemoteSigner },
                modifier = contentModifier,
            )
            AppRoute.RemoteSigner -> RemoteSignerScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Identity },
                onOpenNostrConnect = { route = AppRoute.NostrConnect },
                modifier = contentModifier,
            )
            AppRoute.NostrConnect -> NostrConnectScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.RemoteSigner },
                modifier = contentModifier,
            )
            AppRoute.EditProfile -> EditProfileScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Identity },
                modifier = contentModifier,
            )
            AppRoute.ProviderModels -> ProviderModelSettingsScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
            AppRoute.AgentChat -> AgentChatScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
            AppRoute.NostrConversations -> NostrConversationsScreen(
                snapshot = snapshot,
                bridge = bridge,
                onConversationSelected = { conv ->
                    selectedConversationId = conv.rootEventId
                    route = AppRoute.NostrConversationDetail(conv.rootEventId)
                },
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
            is AppRoute.NostrConversationDetail -> {
                // Look up the conversation by root-event-id from the live snapshot.
                val conv = snapshot?.nostrConversations
                    ?.firstOrNull { it.rootEventId == current.rootEventId }
                if (conv != null) {
                    NostrConversationDetailScreen(
                        conversation = conv,
                        snapshot = snapshot,
                        bridge = bridge,
                        onBack = { route = AppRoute.NostrConversations },
                        modifier = contentModifier,
                    )
                } else {
                    // Conversation cleared from kernel state (tombstone) — go back.
                    Box(modifier = contentModifier, contentAlignment = Alignment.Center) {
                        androidx.compose.material3.Text("Conversation not found.")
                    }
                }
            }
            AppRoute.ClipList -> ClipListScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
            AppRoute.Bookmarks -> BookmarksScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                onEpisodeSelected = { episode ->
                    route = AppRoute.EpisodeDetail(
                        episodeId = episode.id,
                        podcastId = episode.podcastId ?: "",
                    )
                },
                modifier = contentModifier,
            )
            AppRoute.Following -> FollowingScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                onFriendSelected = { hex ->
                    // Look up npub from the live snapshot so the detail screen has a
                    // bech32 fallback label before the kernel resolves kind:0.
                    val npub = snapshot?.following
                        ?.firstOrNull { it.pubkeyHex == hex }?.npub ?: ""
                    route = AppRoute.FriendDetail(hex, npub)
                },
                modifier = contentModifier,
            )
            is AppRoute.FriendDetail -> FriendDetailScreen(
                pubkeyHex = current.pubkeyHex,
                npub = current.npub,
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Following },
                modifier = contentModifier,
            )
            AppRoute.ScheduledTasks -> ScheduledTasksScreen(
                snapshot = snapshot,
                bridge = bridge,
                onBack = { route = AppRoute.Tab(selectedTab) },
                modifier = contentModifier,
            )
        }
    }
}

@Composable
private fun TabContent(
    tab: BottomTab,
    snapshot: PodcastSnapshot?,
    bridge: KernelBridge,
    onShowSelected: (PodcastSummary) -> Unit,
    onOpenIdentity: () -> Unit,
    onOpenModels: () -> Unit,
    onOpenNostrConversations: () -> Unit,
    onOpenClips: () -> Unit,
    onOpenBookmarks: () -> Unit,
    onOpenFollowing: () -> Unit,
    onOpenScheduledTasks: () -> Unit,
    modifier: Modifier,
) {
    when (tab) {
        BottomTab.Home -> HomeScreen(snapshot = snapshot, bridge = bridge, modifier = modifier)
        BottomTab.Search -> SearchScreen(
            snapshot = snapshot,
            bridge = bridge,
            onSubscribed = { showId -> onShowSelected(PodcastSummary(id = showId, title = "")) },
            onResultTapped = onShowSelected,
            modifier = modifier,
        )
        BottomTab.Library -> LibraryScreen(snapshot = snapshot, bridge = bridge, onShowSelected = onShowSelected, modifier = modifier)
        BottomTab.Downloads -> DownloadsScreen(snapshot = snapshot, bridge = bridge, modifier = modifier)
        BottomTab.Inbox -> InboxScreen(snapshot = snapshot, bridge = bridge, modifier = modifier)
        BottomTab.Player -> PlayerScreen(snapshot = snapshot, bridge = bridge, modifier = modifier)
        // Agent tab IS the AgentChatScreen root — onBack navigates Home
        // (consistent with how other full-page tabs reset to Home if somehow
        // backed into from a bottom-nav tap rather than a push surface).
        BottomTab.Agent -> AgentChatScreen(
            snapshot = snapshot,
            bridge = bridge,
            onBack = { /* no-op: already at tab root */ },
            modifier = modifier,
        )
        BottomTab.Settings -> SettingsScreen(
            snapshot = snapshot,
            bridge = bridge,
            onNavigateToIdentity = onOpenIdentity,
            onNavigateToModels = onOpenModels,
            onNavigateToNostrConversations = onOpenNostrConversations,
            onNavigateToClips = onOpenClips,
            onNavigateToBookmarks = onOpenBookmarks,
            onNavigateToFollowing = onOpenFollowing,
            onNavigateToScheduledTasks = onOpenScheduledTasks,
            modifier = modifier,
        )
    }
}

/**
 * Bottom tabs. Order matters — it's the visual sequence in `NavigationBar`.
 * Icon choices match the iOS `TabRouter.swift` mapping while using Material's
 * stock vectors. Inbox is inserted between Downloads and Player to mirror iOS
 * tab ordering.
 */
enum class BottomTab(val label: String, val icon: ImageVector) {
    Home("Home", Icons.Filled.Home),
    Search("Search", Icons.Filled.Search),
    Library("Library", Icons.AutoMirrored.Filled.LibraryBooks),
    Downloads("Downloads", Icons.Filled.Download),
    Inbox("Inbox", Icons.Filled.Inbox),
    Player("Player", Icons.Filled.PlayCircle),
    Agent("Agent", Icons.Filled.AccountBox),
    Settings("Settings", Icons.Filled.Settings),
}

/**
 * Sealed routes the navigator can render. `Saver` is implemented inline so
 * `rememberSaveable` can restore the route through process death.
 *
 * The router stays flat (no nested back-stack) per M13.C+D scope — the only
 * pushes are narrow detail/settings surfaces. M14 may swap this for
 * `androidx.navigation.compose` if deep links land.
 */
internal sealed interface AppRoute {
    data class Tab(val tab: BottomTab) : AppRoute
    data class ShowDetail(val showId: String) : AppRoute
    data class EpisodeDetail(val episodeId: String, val podcastId: String) : AppRoute
    data object Identity : AppRoute
    /** Edit-profile surface — reached from [Identity] when signed in. */
    data object EditProfile : AppRoute
    data object ProviderModels : AppRoute
    data object AgentChat : AppRoute
    /** Nostr conversations list — reached from Settings. */
    data object NostrConversations : AppRoute
    /** Nostr conversation detail — reached from [NostrConversations]. */
    data class NostrConversationDetail(val rootEventId: String) : AppRoute
    /** NIP-46 bunker:// paste-and-connect flow — reached from [Identity]. */
    data object RemoteSigner : AppRoute
    /** NIP-46 nostrconnect:// QR pairing flow — reached from [RemoteSigner]. */
    data object NostrConnect : AppRoute
    /** Global clip list — reachable from Settings (mirrors iOS Clippings tab). */
    data object ClipList : AppRoute
    /** Global bookmarks list — starred episodes, reachable from Settings (mirrors iOS Bookmarks tab). */
    data object Bookmarks : AppRoute
    /** NIP-02 follow list — reached from Settings > Nostr > Following (social parity slice 1). */
    data object Following : AppRoute
    /**
     * Friend detail — reached by tapping a row in [FollowingScreen] (social parity slice 4).
     * [npub] is the bech32-encoded public key, carried so the detail screen can show a
     * short-npub fallback before the kernel resolves the kind:0 profile.
     */
    data class FriendDetail(val pubkeyHex: String, val npub: String) : AppRoute
    /** Scheduled agent tasks list — reached from Settings > Agent (tasks parity slice 3). */
    data object ScheduledTasks : AppRoute

    companion object {
        val Saver: androidx.compose.runtime.saveable.Saver<AppRoute, Any> =
            androidx.compose.runtime.saveable.Saver(
                save = { value -> saveAppRoute(value) },
                restore = { raw ->
                    @Suppress("UNCHECKED_CAST")
                    val list = raw as? List<String> ?: return@Saver null
                    restoreAppRoute(list)
                },
            )
    }
}

/**
 * Encodes an [AppRoute] as a [List<String>] for [rememberSaveable] persistence.
 * Extracted to a top-level function so JVM unit tests can verify the mapping
 * without a Compose [SaverScope].
 */
internal fun saveAppRoute(value: AppRoute): List<String> = when (value) {
    is AppRoute.Tab -> listOf("tab", value.tab.name)
    is AppRoute.ShowDetail -> listOf("show", value.showId)
    is AppRoute.EpisodeDetail -> listOf("episode", value.episodeId, value.podcastId)
    AppRoute.Identity -> listOf("identity")
    AppRoute.EditProfile -> listOf("edit_profile")
    AppRoute.ProviderModels -> listOf("provider_models")
    AppRoute.AgentChat -> listOf("agent_chat")
    AppRoute.NostrConversations -> listOf("nostr_conversations")
    is AppRoute.NostrConversationDetail -> listOf("nostr_conversation_detail", value.rootEventId)
    AppRoute.RemoteSigner -> listOf("remote_signer")
    AppRoute.NostrConnect -> listOf("nostr_connect")
    AppRoute.ClipList -> listOf("clip_list")
    AppRoute.Bookmarks -> listOf("bookmarks")
    AppRoute.Following -> listOf("following")
    is AppRoute.FriendDetail -> listOf("friend_detail", value.pubkeyHex, value.npub)
    AppRoute.ScheduledTasks -> listOf("scheduled_tasks")
}

/**
 * Decodes a [List<String>] produced by [saveAppRoute] back into an [AppRoute].
 * Returns `null` for unrecognised or malformed input so [rememberSaveable] falls
 * back to the initial value rather than crashing on a stale bundle.
 */
internal fun restoreAppRoute(list: List<String>): AppRoute? = when (list.firstOrNull()) {
    "tab" -> AppRoute.Tab(BottomTab.entries.firstOrNull { it.name == list.getOrNull(1) } ?: BottomTab.Home)
    "show" -> list.getOrNull(1)?.let { AppRoute.ShowDetail(it) }
    "episode" -> {
        val ep = list.getOrNull(1)
        val pod = list.getOrNull(2)
        if (ep != null && pod != null) AppRoute.EpisodeDetail(ep, pod) else null
    }
    "identity" -> AppRoute.Identity
    "edit_profile" -> AppRoute.EditProfile
    "provider_models" -> AppRoute.ProviderModels
    "agent_chat" -> AppRoute.AgentChat
    "nostr_conversations" -> AppRoute.NostrConversations
    "nostr_conversation_detail" -> list.getOrNull(1)?.let { AppRoute.NostrConversationDetail(it) }
    "remote_signer" -> AppRoute.RemoteSigner
    "nostr_connect" -> AppRoute.NostrConnect
    "clip_list" -> AppRoute.ClipList
    "bookmarks" -> AppRoute.Bookmarks
    "following" -> AppRoute.Following
    "friend_detail" -> {
        val hex = list.getOrNull(1)
        val npub = list.getOrNull(2)
        if (hex != null && npub != null) AppRoute.FriendDetail(hex, npub) else null
    }
    "scheduled_tasks" -> AppRoute.ScheduledTasks
    else -> null
}
