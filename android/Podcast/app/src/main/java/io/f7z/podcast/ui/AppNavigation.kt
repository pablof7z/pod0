package io.f7z.podcast.ui

import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.LibraryBooks
import androidx.compose.material.icons.filled.Download
import androidx.compose.material.icons.filled.Home
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
import io.f7z.podcast.PodcastSnapshot
import io.f7z.podcast.PodcastSummary

/**
 * Bottom-tab navigator wiring Home / Library / Player / Settings + an
 * inline stack for nested surfaces (ShowDetail, Identity) reached from
 * a tab.
 *
 * Why hand-rolled instead of `androidx.navigation.compose`?
 *
 *  * The M13.C+D scope is four tabs + two pushable surfaces. A nav-compose
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
 *    to their root; tapping a library tile pushes to `ShowDetail`; tapping
 *    "Account" in Settings pushes to `Identity`.
 */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun AppNavigation(snapshot: PodcastSnapshot?, bridge: KernelBridge) {
    var selectedTab by rememberSaveable { mutableStateOf(BottomTab.Home) }
    var route by rememberSaveable(stateSaver = AppRoute.Saver) { mutableStateOf<AppRoute>(AppRoute.Tab(BottomTab.Home)) }

    val onTabSelected: (BottomTab) -> Unit = { tab ->
        selectedTab = tab
        route = AppRoute.Tab(tab)
    }

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
                onSignInRequested = { /* No-op stub — toast surface is M13.B scope. */ },
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
        BottomTab.Player -> PlayerScreen(snapshot = snapshot, bridge = bridge, modifier = modifier)
        BottomTab.Settings -> SettingsScreen(snapshot = snapshot, bridge = bridge, onNavigateToIdentity = onOpenIdentity, modifier = modifier)
    }
}

/**
 * Four bottom tabs. Order matters — it's the visual sequence in
 * `NavigationBar`. Icon choices match the iOS `TabRouter.swift` mapping
 * (house / library / play / gear) while using Material's stock vectors.
 */
enum class BottomTab(val label: String, val icon: ImageVector) {
    Home("Home", Icons.Filled.Home),
    Search("Search", Icons.Filled.Search),
    Library("Library", Icons.AutoMirrored.Filled.LibraryBooks),
    Downloads("Downloads", Icons.Filled.Download),
    Player("Player", Icons.Filled.PlayCircle),
    Settings("Settings", Icons.Filled.Settings),
}

/**
 * Sealed routes the navigator can render. `Saver` is implemented inline so
 * `rememberSaveable` can restore the route through process death.
 *
 * The router stays flat (no nested back-stack) per M13.C+D scope — the only
 * pushes are ShowDetail (from Library) and Identity (from Settings). M14
 * may swap this for `androidx.navigation.compose` if deep links land.
 */
private sealed interface AppRoute {
    data class Tab(val tab: BottomTab) : AppRoute
    data class ShowDetail(val showId: String) : AppRoute
    data class EpisodeDetail(val episodeId: String, val podcastId: String) : AppRoute
    data object Identity : AppRoute

    companion object {
        val Saver: androidx.compose.runtime.saveable.Saver<AppRoute, Any> =
            androidx.compose.runtime.saveable.Saver(
                save = { value ->
                    when (value) {
                        is Tab -> listOf("tab", value.tab.name)
                        is ShowDetail -> listOf("show", value.showId)
                        is EpisodeDetail -> listOf("episode", value.episodeId, value.podcastId)
                        Identity -> listOf("identity")
                    }
                },
                restore = { raw ->
                    @Suppress("UNCHECKED_CAST")
                    val list = raw as? List<String> ?: return@Saver null
                    when (list.firstOrNull()) {
                        "tab" -> Tab(BottomTab.entries.firstOrNull { it.name == list.getOrNull(1) } ?: BottomTab.Home)
                        "show" -> list.getOrNull(1)?.let { ShowDetail(it) }
                        "episode" -> {
                            val ep = list.getOrNull(1)
                            val pod = list.getOrNull(2)
                            if (ep != null && pod != null) EpisodeDetail(ep, pod) else null
                        }
                        "identity" -> Identity
                        else -> null
                    }
                },
            )
    }
}
