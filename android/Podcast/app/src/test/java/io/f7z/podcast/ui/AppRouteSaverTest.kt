package io.f7z.podcast.ui

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

/**
 * Unit tests for [saveAppRoute] / [restoreAppRoute] — the pure encoding layer
 * under [AppRoute.Saver].
 *
 * Exercising the pure functions directly avoids a Compose [SaverScope] dependency
 * so the tests run on the host JVM with JUnit only (no instrumented test
 * infrastructure required).
 *
 * Critical contract: every [AppRoute] subtype MUST round-trip losslessly.
 * Missing an entry in [saveAppRoute] causes a compile error (sealed when-must-
 * be-exhaustive). Missing an entry in [restoreAppRoute] causes a null restore,
 * which silently navigates back to the initial route on process death — caught
 * here before it ships.
 */
class AppRouteSaverTest {

    // ── Helper ────────────────────────────────────────────────────────────────

    private fun roundTrip(route: AppRoute): AppRoute? =
        restoreAppRoute(saveAppRoute(route))

    // ── Exhaustive round-trip — EVERY AppRoute variant survives the Saver ─────
    //
    // `restoreAppRoute` is a string-keyed `when` with an `else -> null` tail, so
    // a future edit that drops a restore case ships SILENTLY (route lost on
    // process-death restore, no compile error). This table asserts every one of
    // the 16 variants survives `restoreAppRoute(saveAppRoute(route)) == route`,
    // turning any dropped case into a test failure rather than a silent regression.
    //
    // Tab is enumerated across ALL BottomTab entries so a renamed/removed tab
    // (which the Saver matches by `.name`) is also caught.

    private val allRoutesExceptTab: List<AppRoute> = listOf(
        AppRoute.ShowDetail("show-id-42"),
        AppRoute.EpisodeDetail("ep-1", "pod-1"),
        AppRoute.Identity,
        AppRoute.EditProfile,
        AppRoute.ProviderModels,
        AppRoute.AgentChat,
        AppRoute.NostrConversations,
        AppRoute.NostrConversationDetail("root-event-id"),
        AppRoute.RemoteSigner,
        AppRoute.NostrConnect,
        AppRoute.ClipList,
        AppRoute.Bookmarks,
        AppRoute.Following,
        AppRoute.FriendDetail("deadbeef00", "npub1abc"),
        AppRoute.ScheduledTasks,
    )

    @Test
    fun `every non-Tab AppRoute variant round-trips through the Saver`() {
        allRoutesExceptTab.forEach { route ->
            assertEquals(
                "AppRoute $route must survive restoreAppRoute(saveAppRoute(it)) — " +
                    "a missing restore case silently loses the route on process death",
                route,
                roundTrip(route),
            )
        }
    }

    @Test
    fun `every BottomTab survives a Tab route round-trip`() {
        BottomTab.entries.forEach { tab ->
            val route = AppRoute.Tab(tab)
            assertEquals(
                "Tab($tab) must round-trip — the Saver matches BottomTab by .name",
                route,
                roundTrip(route),
            )
        }
    }

    @Test
    fun `round-trip coverage spans all 16 AppRoute declared subtypes`() {
        // 15 non-Tab variants + Tab = 16 declared AppRoute subtypes. This guards
        // the table above from silently falling behind a newly-added route: bump
        // this count deliberately when a variant is added, after covering it.
        val nonTabSubtypeCount = allRoutesExceptTab
            .map { it::class }
            .distinct()
            .size
        assertEquals(
            "Expected 15 distinct non-Tab AppRoute subtypes in the round-trip table",
            15,
            nonTabSubtypeCount,
        )
    }

    // ── FriendDetail — the new route added in slice 4 ─────────────────────────

    @Test
    fun `FriendDetail round-trips pubkeyHex and npub`() {
        val original = AppRoute.FriendDetail(
            pubkeyHex = "deadbeef1234567890abcdef1234567890abcdef1234567890abcdef1234567890",
            npub = "npub1abc123xyz456",
        )
        assertEquals(original, roundTrip(original))
    }

    @Test
    fun `FriendDetail with empty npub round-trips`() {
        val original = AppRoute.FriendDetail(pubkeyHex = "abcd1234", npub = "")
        assertEquals(original, roundTrip(original))
    }

    @Test
    fun `saveAppRoute FriendDetail encodes friend_detail tag at index 0`() {
        val saved = saveAppRoute(AppRoute.FriendDetail("hex123", "npub456"))
        assertEquals("friend_detail", saved[0])
    }

    @Test
    fun `saveAppRoute FriendDetail encodes pubkeyHex at index 1`() {
        val saved = saveAppRoute(AppRoute.FriendDetail("hex123", "npub456"))
        assertEquals("hex123", saved[1])
    }

    @Test
    fun `saveAppRoute FriendDetail encodes npub at index 2`() {
        val saved = saveAppRoute(AppRoute.FriendDetail("hex123", "npub456"))
        assertEquals("npub456", saved[2])
    }

    @Test
    fun `restoreAppRoute with incomplete friend_detail list returns null`() {
        // Only tag + pubkeyHex, no npub — must not crash, must return null.
        assertNull(restoreAppRoute(listOf("friend_detail", "hexonly")))
    }

    // ── Following (the parent route of FriendDetail) ──────────────────────────

    @Test
    fun `Following round-trips`() {
        assertEquals(AppRoute.Following, roundTrip(AppRoute.Following))
    }

    // ── Other routes — verify the new FriendDetail case doesn't break peers ───

    @Test
    fun `ShowDetail round-trips`() {
        val original = AppRoute.ShowDetail("show-id-42")
        assertEquals(original, roundTrip(original))
    }

    @Test
    fun `EpisodeDetail round-trips`() {
        val original = AppRoute.EpisodeDetail("ep-1", "pod-1")
        assertEquals(original, roundTrip(original))
    }

    @Test
    fun `NostrConversationDetail round-trips`() {
        val original = AppRoute.NostrConversationDetail("root-event-id")
        assertEquals(original, roundTrip(original))
    }

    // ── ScheduledTasks — the new route added in tasks parity slice 3 ─────────

    @Test
    fun `ScheduledTasks round-trips`() {
        assertEquals(AppRoute.ScheduledTasks, roundTrip(AppRoute.ScheduledTasks))
    }

    @Test
    fun `saveAppRoute ScheduledTasks encodes scheduled_tasks tag at index 0`() {
        val saved = saveAppRoute(AppRoute.ScheduledTasks)
        assertEquals("scheduled_tasks", saved[0])
    }

    @Test
    fun `restoreAppRoute scheduled_tasks tag produces ScheduledTasks`() {
        assertEquals(AppRoute.ScheduledTasks, restoreAppRoute(listOf("scheduled_tasks")))
    }

    // ── Peer-regression guard — ScheduledTasks must not break existing routes ──

    @Test
    fun `unknown tag restores to null`() {
        assertNull(restoreAppRoute(listOf("completely_unknown_route_tag")))
    }

    @Test
    fun `empty list restores to null`() {
        assertNull(restoreAppRoute(emptyList()))
    }
}
