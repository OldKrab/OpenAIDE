package io.openaide.android

/**
 * Decides when the connection in use has stopped receiving from the computer
 * while the app is on screen. Kept free of Android and of the library so the
 * rules can be tested; [RemoteNode] feeds it the connection's received-datagram
 * count.
 *
 * The library keeps a connection alive with a heartbeat every few seconds, so a
 * healthy connection receives something well within [STALL_AFTER_MS] even when
 * the workspace is idle. A connection that only sends is the failure this
 * reports: the user sees "Reconnecting…" while the connection still looks open.
 * Healthy connections stay quiet; each stall yields one report and one end.
 */
internal class ReceiveWatch {
    sealed interface Change {
        /** Nothing has arrived for [silentMs] while on screen. */
        data class Stalled(val silentMs: Long) : Change
        /** Data arrived again after [silentMs] of silence. */
        data class Resumed(val silentMs: Long) : Change
        /** The app left the screen during a stall that had lasted [silentMs]. */
        data class Hidden(val silentMs: Long) : Change
        /** The connection ended or was replaced during a stall that had lasted [silentMs]. */
        data class Ended(val silentMs: Long) : Change
    }

    /** When something was last received, or when watching started over; null while hidden. */
    private var quietSince: Long? = null
    private var lastRx = -1L
    private var reported = false

    /** One sample of the received-datagram count at [nowMs]. */
    fun sample(nowMs: Long, rx: Long, visible: Boolean): Change? {
        if (!visible) {
            val since = quietSince
            quietSince = null
            lastRx = rx
            return if (reported && since != null) { reported = false; Change.Hidden(nowMs - since) } else null
        }
        val since = quietSince
        if (since == null || rx != lastRx) {
            lastRx = rx
            quietSince = nowMs
            if (reported && since != null) { reported = false; return Change.Resumed(nowMs - since) }
            return null
        }
        if (!reported && nowMs - since >= STALL_AFTER_MS) { reported = true; return Change.Stalled(nowMs - since) }
        return null
    }

    /** The connection is no longer watched. */
    fun end(nowMs: Long): Change? {
        val since = quietSince ?: return null
        if (!reported) return null
        reported = false
        return Change.Ended(nowMs - since)
    }

    companion object {
        /** Two missed heartbeats of the library and then some: not a slow round trip. */
        const val STALL_AFTER_MS = 12_000L
        const val SAMPLE_MS = 2_000L
    }
}
