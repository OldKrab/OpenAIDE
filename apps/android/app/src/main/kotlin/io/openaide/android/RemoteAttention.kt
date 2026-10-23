package io.openaide.android

import android.content.Context
import android.os.SystemClock
import java.security.SecureRandom
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.json.JSONObject

/**
 * Follows the paired computer's Task Attention Events for the background service
 * (ADR-0027, ADR-0062), because the WebView's JavaScript is paused while hidden.
 *
 * It is an App Server client of its own, under an id only this process uses, so it
 * never competes with the workspace's client. It keeps one `taskAttention/wait` in
 * flight over the [RemoteNode] connection the workspace also uses, each on its own
 * stream. A failed exchange is retried at once, then at growing intervals; only a
 * failure that persists past the first retry is reported, so the connection that
 * a return to the screen replaces stays quiet.
 *
 * The retry delays run on the uptime clock: while the phone sleeps they pause, and
 * a lost connection is retried when Android next wakes the app. An answer arriving
 * on a live connection is what wakes it.
 */
internal class RemoteAttention(private val context: Context, private val events: Events) {
    /** What the watch can tell the user. Every state after [RECONNECTING] ends the watch. */
    enum class State { CONNECTING, WATCHING, RECONNECTING, NOT_TRUSTED, INCOMPATIBLE, OUTDATED, UNAVAILABLE }

    /** Called on the watch's own thread. */
    interface Events {
        fun state(state: State)
        fun attention(current: List<AttentionNotifications.Attention>)
    }

    private class Retry(val reason: String) : Exception(reason)
    private class Ended(val state: State) : Exception(state.name)

    private val clientId = "android-attention-" + ByteArray(8).also(SecureRandom()::nextBytes)
        .joinToString("") { "%02x".format(it.toInt() and 0xff) }
    private val requests = AtomicInteger()
    private val diagnostics = AndroidDiagnostics()
    @Volatile private var thread: Thread? = null

    @Synchronized fun start() {
        if (thread != null) return
        // Assigned before it runs: the watch lasts while it is the current one.
        val watching = Thread(::watch, "openaide-remote-attention").apply { isDaemon = true }
        thread = watching
        watching.start()
    }

    @Synchronized fun stop() {
        thread?.interrupt()
        thread = null
    }

    private fun watch() {
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("remote_attention", "started", 0, 0)
        events.state(State.CONNECTING)
        var cursor: String? = null
        var initialized = false
        var failures = 0
        var failingSince = 0L
        var outcome = "stopped"
        while (thread === Thread.currentThread()) {
            try {
                if (!initialized) {
                    exchange("client/initialize", AttentionWire.initializeParams(clientId), INITIALIZE_TIMEOUT_MS)
                    initialized = true
                }
                val answer = AttentionWire.waitResult(exchange(AttentionWire.WAIT, AttentionWire.waitParams(cursor), WAIT_TIMEOUT_MS))
                    ?: throw Retry("invalid_result")
                if (cursor == null) {
                    // Only the first answer after a start or a failure; held answers stay quiet.
                    diagnostics.record("remote_attention", "watching", failures, if (failures > 0) SystemClock.elapsedRealtime() - failingSince else 0)
                    events.state(State.WATCHING)
                }
                failures = 0
                cursor = answer.cursor
                events.attention(answer.attention)
            } catch (retry: Retry) {
                // The next exchange starts over: App Server may have restarted and forgotten this client.
                initialized = false
                cursor = null
                if (++failures == 1) failingSince = SystemClock.elapsedRealtime()
                if (failures == 2) events.state(State.RECONNECTING)
                if (failures == 2 || failures % 10 == 0) {
                    diagnostics.record("remote_attention", "retrying", failures, SystemClock.elapsedRealtime() - failingSince,
                        mapOf("cause" to retry.reason))
                }
                try { Thread.sleep(AttentionWire.retryDelayMs(failures)) } catch (stopped: InterruptedException) { break }
            } catch (ended: Ended) {
                outcome = ended.state.name.lowercase()
                events.state(ended.state)
                break
            } catch (stopped: InterruptedException) {
                break
            }
        }
        diagnostics.record("remote_attention", outcome, failures, SystemClock.elapsedRealtime() - started)
    }

    /** One request on a new stream, answered or classified as a retry or an end. */
    private fun exchange(method: String, params: JSONObject, timeoutMs: Long): JSONObject {
        val id = "android-attention-${requests.incrementAndGet()}"
        val request = AttentionWire.request(clientId, id, method, params)
        val response = try {
            runBlocking {
                withTimeout(timeoutMs) {
                    val connection = RemoteNode.connection(context)
                    val stream = try { connection.openBi() } catch (error: Throwable) {
                        throw RemoteNode.Unavailable(RemoteNode.drop(connection))
                    }
                    stream.send().writeAll(request)
                    stream.send().finish()
                    stream.recv().readToEnd(AttentionWire.MAX_RESPONSE_BYTES.toUInt())
                }
            }
        } catch (error: RemoteNode.Unavailable) {
            throw when (error.failure) {
                RemoteNode.Failure.UNREACHABLE -> Retry("unreachable")
                RemoteNode.Failure.NOT_TRUSTED -> Ended(State.NOT_TRUSTED)
                RemoteNode.Failure.INCOMPATIBLE -> Ended(State.INCOMPATIBLE)
                RemoteNode.Failure.STORAGE -> Ended(State.UNAVAILABLE)
            }
        } catch (error: TimeoutCancellationException) {
            throw Retry("timeout")
        } catch (error: InterruptedException) {
            throw error
        } catch (error: Throwable) {
            throw Retry("stream_failed")
        }
        return when (val answer = AttentionWire.answer(response, id, method)) {
            is AttentionWire.Answer.Result -> answer.value
            AttentionWire.Answer.NotInitialized -> throw Retry("not_initialized")
            AttentionWire.Answer.Unsupported -> throw Ended(State.OUTDATED)
            is AttentionWire.Answer.Failed -> throw Retry(answer.cause)
        }
    }

    private companion object {
        /** Covers a slow dial and App Server starting up. */
        const val INITIALIZE_TIMEOUT_MS = 60_000L
        /** App Server answers a held wait within 25 seconds; past this the request is lost. */
        const val WAIT_TIMEOUT_MS = 45_000L
    }
}
