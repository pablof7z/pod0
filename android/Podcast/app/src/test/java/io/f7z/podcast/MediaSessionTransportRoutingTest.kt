package io.f7z.podcast

import androidx.media3.common.Player
import io.f7z.podcast.capabilities.KernelForwardingPlayer
import kotlinx.serialization.json.Json
import kotlinx.serialization.json.jsonObject
import kotlinx.serialization.json.jsonPrimitive
import org.junit.Test
import org.mockito.Mockito.mock
import org.mockito.Mockito.verify
import org.junit.Assert.assertEquals

/**
 * Unit tests for [KernelForwardingPlayer] transport routing.
 *
 * Verifies that when a [KernelDispatcher] is set, media3 transport commands
 * (play/pause/seekTo/seekForward/seekBack) dispatch to the kernel instead of
 * executing directly on the inner player. With bridge = null, commands fall
 * back to the inner player.
 *
 * Uses Mockito to stub the media3 [Player] interface (many abstract methods;
 * a manual full implementation would be brittle). Uses [FakeDispatcher] (not
 * [KernelBridge]) as the bridge double — [KernelBridge]'s init block loads
 * the native `.so` via System.loadLibrary, which is unavailable in JVM tests.
 */
class MediaSessionTransportRoutingTest {

    private val json = Json

    // MARK: - Bridge routing tests

    @Test
    fun playWithBridgeDispatchesResume() {
        val forwarder = KernelForwardingPlayer(mock(Player::class.java))
        val dispatcher = FakeDispatcher()
        forwarder.bridge = dispatcher

        forwarder.play()

        assertEquals(1, dispatcher.actions.size)
        val payload = json.parseToJsonElement(dispatcher.actions[0].payload).jsonObject
        assertEquals("podcast.player", dispatcher.actions[0].namespace)
        assertEquals("resume", payload["op"]?.jsonPrimitive?.content)
    }

    @Test
    fun pauseWithBridgeDispatchesPause() {
        val forwarder = KernelForwardingPlayer(mock(Player::class.java))
        val dispatcher = FakeDispatcher()
        forwarder.bridge = dispatcher

        forwarder.pause()

        assertEquals(1, dispatcher.actions.size)
        val payload = json.parseToJsonElement(dispatcher.actions[0].payload).jsonObject
        assertEquals("pause", payload["op"]?.jsonPrimitive?.content)
    }

    @Test
    fun seekToWithBridgeDispatchesSeek() {
        val forwarder = KernelForwardingPlayer(mock(Player::class.java))
        val dispatcher = FakeDispatcher()
        forwarder.bridge = dispatcher

        forwarder.seekTo(0, 90_000L) // 90 seconds in milliseconds

        assertEquals(1, dispatcher.actions.size)
        val payload = json.parseToJsonElement(dispatcher.actions[0].payload).jsonObject
        assertEquals("seek", payload["op"]?.jsonPrimitive?.content)
        assertEquals(90.0, payload["position_secs"]?.jsonPrimitive?.content?.toDouble())
    }

    @Test
    fun seekForwardWithBridgeDispatchesSkipForward() {
        val forwarder = KernelForwardingPlayer(mock(Player::class.java))
        val dispatcher = FakeDispatcher()
        forwarder.bridge = dispatcher

        forwarder.seekForward()

        // seekForward dispatches two actions: first a seek (position sync),
        // then the skip_forward, so consecutive remote skips while paused
        // accumulate from the correct base in the Rust PlayerActor.
        assertEquals(2, dispatcher.actions.size)
        val seekPayload = json.parseToJsonElement(dispatcher.actions[0].payload).jsonObject
        assertEquals("seek", seekPayload["op"]?.jsonPrimitive?.content)
        assertEquals(0.0, seekPayload["position_secs"]?.jsonPrimitive?.content?.toDouble())
        val skipPayload = json.parseToJsonElement(dispatcher.actions[1].payload).jsonObject
        assertEquals("skip_forward", skipPayload["op"]?.jsonPrimitive?.content)
    }

    @Test
    fun seekBackWithBridgeDispatchesSkipBackward() {
        val forwarder = KernelForwardingPlayer(mock(Player::class.java))
        val dispatcher = FakeDispatcher()
        forwarder.bridge = dispatcher

        forwarder.seekBack()

        // seekBack dispatches two actions: first a seek (position sync),
        // then the skip_backward, matching the same paused-accumulation fix.
        assertEquals(2, dispatcher.actions.size)
        val seekPayload = json.parseToJsonElement(dispatcher.actions[0].payload).jsonObject
        assertEquals("seek", seekPayload["op"]?.jsonPrimitive?.content)
        assertEquals(0.0, seekPayload["position_secs"]?.jsonPrimitive?.content?.toDouble())
        val skipPayload = json.parseToJsonElement(dispatcher.actions[1].payload).jsonObject
        assertEquals("skip_backward", skipPayload["op"]?.jsonPrimitive?.content)
    }

    // MARK: - Fallback tests (bridge = null)

    @Test
    fun playWithoutBridgeFallsBackToInnerPlayer() {
        val innerPlayer = mock(Player::class.java)
        val forwarder = KernelForwardingPlayer(innerPlayer)
        // bridge = null (default)

        forwarder.play()

        verify(innerPlayer).play()
    }

    @Test
    fun pauseWithoutBridgeFallsBackToInnerPlayer() {
        val innerPlayer = mock(Player::class.java)
        val forwarder = KernelForwardingPlayer(innerPlayer)
        // bridge = null

        forwarder.pause()

        verify(innerPlayer).pause()
    }

    @Test
    fun seekToWithoutBridgeFallsBackToInnerPlayer() {
        val innerPlayer = mock(Player::class.java)
        val forwarder = KernelForwardingPlayer(innerPlayer)
        // bridge = null

        forwarder.seekTo(0, 60_000L) // 60 seconds

        verify(innerPlayer).seekTo(0, 60_000L)
    }
}

// MARK: - Fakes

private data class DispatchedAction(
    val namespace: String,
    val payload: String,
)

/**
 * Fake [KernelDispatcher] for testing that records dispatch calls.
 *
 * Implements [KernelDispatcher] (the thin interface extracted for testability),
 * NOT [KernelBridge] (which loads a native .so in its init block).
 */
private class FakeDispatcher : KernelDispatcher {
    val actions = mutableListOf<DispatchedAction>()

    override fun dispatchAction(namespace: String, payloadJson: String): String? {
        actions.add(DispatchedAction(namespace, payloadJson))
        return null
    }
}
