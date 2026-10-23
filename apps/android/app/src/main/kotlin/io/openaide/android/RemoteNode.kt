package io.openaide.android

import android.content.Context
import android.os.SystemClock
import computer.iroh.Connection
import computer.iroh.ConnectionStats
import computer.iroh.Endpoint
import computer.iroh.EndpointAddr
import computer.iroh.EndpointId
import computer.iroh.EndpointOptions
import computer.iroh.IrohAndroid
import computer.iroh.SecretKey
import computer.iroh.presetN0
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.TimeoutCancellationException
import kotlinx.coroutines.delay
import kotlinx.coroutines.launch
import kotlinx.coroutines.sync.Mutex
import kotlinx.coroutines.sync.withLock
import kotlinx.coroutines.withTimeout
import kotlinx.coroutines.withTimeoutOrNull
import org.json.JSONObject

/**
 * This phone as a Remote Device: its key pair, its iroh endpoint, and the one
 * connection to the paired computer's App Server (ADR-0062).
 *
 * One endpoint is in use at a time: the library closes every connection when an
 * endpoint is collected, and two endpoints with the same key compete for its
 * relay. It is replaced, not kept for the life of the process, because Android
 * stops a hidden app's network and the library cannot observe that on Android.
 * See [ReconnectPolicy].
 */
internal object RemoteNode {
    /** Why the paired computer cannot be used right now. */
    enum class Failure { UNREACHABLE, NOT_TRUSTED, INCOMPATIBLE, STORAGE }

    class Unavailable(val failure: Failure) : Exception(failure.name)

    private val APP_ALPN = "openaide/app-server/${BuildConfig.PROTOCOL_MAJOR}".toByteArray()
    private val PAIRING_ALPN = "openaide/pair/1".toByteArray()
    private const val PAIRING_MESSAGE_LIMIT = 4096u
    private const val PAIRING_TIMEOUT_MS = 20_000L

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val diagnostics = AndroidDiagnostics()
    private val binding = Mutex()
    private val connecting = Mutex()
    private var endpoint: Endpoint? = null
    /** Why the endpoint must be replaced before its next use, or null. Set from any thread, acted on under [binding]. */
    @Volatile private var stale: String? = null
    @Volatile private var hiddenAt = 0L
    private var binds = 0
    /** Set by the first bind, for the bind that follows a return while a join is awaited. */
    @Volatile private var application: Context? = null
    @Volatile private var connection: Connection? = null
    @Volatile private var joinListener: ((ConnectionStore.PairedServer) -> Unit)? = null
    private var attempts = 0

    /** The app left the screen; Android may stop its network from here on. */
    fun hidden() { hiddenAt = SystemClock.elapsedRealtime() }

    /**
     * The app is on screen again. After more than a short pause the endpoint is
     * replaced before the next dial, and a dial already waiting on it is ended.
     */
    fun shown() {
        val since = hiddenAt
        hiddenAt = 0
        if (since == 0L || !ReconnectPolicy.staleAfterHidden(SystemClock.elapsedRealtime() - since)) return
        stale = "resumed"
        scope.launch {
            // A computer answering this phone's own code connects to the endpoint, so
            // one must be listening again at once; otherwise the next dial binds it.
            val waiting = application.takeIf { joinListener != null }
            if (waiting != null) runCatching { endpoint(waiting) } else binding.withLock { retire() }
        }
    }

    /**
     * Gives up the endpoint marked stale and its connection. Closing can take
     * seconds on a dead network, so it does not delay the endpoint that follows.
     */
    private fun retire() {
        val reason = stale ?: return
        val old = endpoint
        endpoint = null
        connection = null
        // Cleared last: until then no caller may take the connection being given up.
        stale = null
        if (old == null) return
        diagnostics.record("remote_endpoint_retire", reason, binds, 0)
        scope.launch {
            val started = SystemClock.elapsedRealtime()
            val outcome = if (runCatching { old.shutdown() }.isSuccess) "closed" else "failed"
            diagnostics.record("remote_endpoint_close", outcome, 0, SystemClock.elapsedRealtime() - started)
        }
    }

    /** Binds the endpoint with this phone's stored key, creating the key on first use. */
    suspend fun endpoint(context: Context): Endpoint = binding.withLock {
        retire()
        endpoint?.let { return it }
        val started = SystemClock.elapsedRealtime()
        val bind = ++binds
        diagnostics.record("remote_endpoint_bind", "started", bind, 0)
        val application = context.applicationContext.also { this.application = it }
        try {
            val store = ConnectionStore(application)
            val key = try {
                store.deviceKey() ?: SecretKey.generate().toBytes().also(store::saveDeviceKey)
            } catch (error: RuntimeException) {
                throw Unavailable(Failure.STORAGE)
            }
            IrohAndroid.installAndroidContext(application)
            val bound = Endpoint.bind(EndpointOptions(preset = presetN0(), secretKey = key, alpns = listOf(PAIRING_ALPN)))
            endpoint = bound
            scope.launch { acceptAnnouncements(application, bound) }
            diagnostics.record("remote_endpoint_bind", "ready", bind, SystemClock.elapsedRealtime() - started)
            bound
        } catch (error: Throwable) {
            diagnostics.record("remote_endpoint_bind", if (error is Unavailable) "storage_unavailable" else "failed", bind,
                SystemClock.elapsedRealtime() - started)
            throw error as? Unavailable ?: Unavailable(Failure.UNREACHABLE)
        }
    }

    /** The Pairing Code this phone shows so a trusted client can add it. */
    suspend fun joinCode(context: Context, name: String, model: String?): String =
        PairingCode.join(endpoint(context).id().toBytes(), name, model)

    /**
     * Listens for the App Server that approved this phone's join request. Only one
     * announcement is accepted, and only while the code is on screen: the code is
     * the only place this phone's key is disclosed.
     */
    fun awaitJoin(listener: (ConnectionStore.PairedServer) -> Unit) { joinListener = listener }

    fun stopJoin() { joinListener = null }

    /** Spends an invite: connects to the App Server it names and presents its secret. */
    suspend fun redeem(context: Context, invite: PairingCode.Invite, name: String, model: String?): ConnectionStore.PairedServer {
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("remote_pairing_invite", "started", 0, 0)
        var outcome = "unreachable"
        try {
            val node = endpoint(context)
            val result = withTimeout(PAIRING_TIMEOUT_MS) {
                val pairing = node.connect(EndpointAddr(EndpointId.fromString(invite.serverId), null, emptyList()), PAIRING_ALPN)
                try {
                    val stream = pairing.openBi()
                    val request = JSONObject().put("secret", invite.secretText).put("name", name)
                    if (!model.isNullOrEmpty()) request.put("model", model)
                    stream.send().writeAll(request.toString().toByteArray())
                    stream.send().finish()
                    JSONObject(String(stream.recv().readToEnd(PAIRING_MESSAGE_LIMIT)))
                } finally {
                    runCatching { pairing.close(0L, "done".toByteArray()) }
                }
            }
            if (!result.optBoolean("trusted")) {
                outcome = "refused"
                throw Unavailable(Failure.NOT_TRUSTED)
            }
            outcome = "trusted"
            return ConnectionStore.PairedServer(invite.serverId, result.optString("serverName"))
        } catch (error: Unavailable) {
            throw error
        } catch (error: Throwable) {
            throw Unavailable(Failure.UNREACHABLE)
        } finally {
            diagnostics.record("remote_pairing_invite", outcome, 0, SystemClock.elapsedRealtime() - started)
        }
    }

    /** The live connection to the paired App Server, opened or reopened on demand. */
    suspend fun connection(context: Context): Connection {
        live()?.let { return it }
        return connecting.withLock {
            live()?.let { return it }
            val store = ConnectionStore(context.applicationContext)
            val server = store.pairedServer() ?: throw Unavailable(Failure.NOT_TRUSTED)
            reach(context, store, server).also { use(it) }
        }
    }

    /**
     * Reaches [server] for a switch while the connection in use stays live: the
     * result is not used until [adopt] takes it, and a failed attempt keeps the
     * endpoint that connection rides on.
     */
    suspend fun reachForSwitch(context: Context, server: ConnectionStore.PairedServer): Connection =
        reach(context, ConnectionStore(context.applicationContext), server)

    /** Makes a connection reached for a switch the one in use, ending the previous one. */
    fun adopt(next: Connection) {
        val previous = connection
        use(next)
        if (previous != null && previous !== next) runCatching { previous.close(0L, "switched".toByteArray()) }
        diagnostics.record("remote_connect", "adopted", attempts, 0)
    }

    /** Makes [opened] the live connection; its receive watch lasts while it stays live. */
    private fun use(opened: Connection) {
        connection = opened
        watchReceive(opened, attempts)
    }

    /** Ends a connection reached for a switch that was cancelled or could not be saved. */
    fun abandon(unused: Connection) {
        if (connection === unused) return
        runCatching { unused.close(0L, "cancelled".toByteArray()) }
        diagnostics.record("remote_connect", "abandoned", attempts, 0)
    }

    /**
     * One slow or stalled dial is not yet an unreachable computer: each further
     * attempt starts on a new endpoint, and the caller keeps showing "Connecting…".
     */
    private suspend fun reach(context: Context, store: ConnectionStore, server: ConnectionStore.PairedServer): Connection {
        val started = SystemClock.elapsedRealtime()
        var index = 0
        while (true) {
            val timeoutMs = ReconnectPolicy.attemptTimeoutMs(index, SystemClock.elapsedRealtime() - started)
            try {
                return dial(context, store, server, index, timeoutMs)
            } catch (error: Unavailable) {
                index++
                // A hidden app usually has no network to retry on; its return starts over anyway.
                val more = hiddenAt == 0L && ReconnectPolicy.attemptTimeoutMs(index, SystemClock.elapsedRealtime() - started) > 0
                if (error.failure != Failure.UNREACHABLE || !more) throw error
                // TODO: a dial for a switch shares the one endpoint with the live connection, so the
                // live connection's own reconnect (a return to the screen, a stalled receive) retires
                // that endpoint and ends the switch's dial mid-attempt, which the user sees as a failed
                // switch to a reachable computer. Give the switch's dial its own endpoint, or have that
                // reconnect stand down while a switch is pending.
                if (ReconnectPolicy.retireAfterFailedDial(live() != null)) stale = "dial_failed"
            }
        }
    }

    /** One attempt to reach the computer, on the current endpoint. */
    private suspend fun dial(context: Context, store: ConnectionStore, server: ConnectionStore.PairedServer, index: Int, timeoutMs: Long): Connection {
        val attempt = ++attempts
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("remote_connect", "started", attempt, 0)
        var outcome = "unreachable"
        try {
            val node = endpoint(context)
            val relay = ReconnectPolicy.relayFor(index, runCatching { store.relay(server.id) }.getOrNull())
            // Says whether this attempt had to look the computer's address up first.
            diagnostics.record("remote_connect_address", if (relay != null) "remembered_relay" else "lookup", attempt, 0)
            val id = EndpointId.fromString(server.id)
            val opened = try {
                withTimeout(timeoutMs) { node.connect(EndpointAddr(id, relay, emptyList()), APP_ALPN) }
            } catch (error: TimeoutCancellationException) {
                outcome = "timeout"
                throw Unavailable(Failure.UNREACHABLE)
            }
            // The App Server completes the handshake before it checks the key,
            // so a removed phone learns it from the close that follows.
            val refusal = withTimeoutOrNull(REFUSAL_WAIT_MS) { opened.closed() }
            if (refusal != null) throw Unavailable(failure(refusal))
            outcome = if (opened.paths().any { it.isSelected && it.isRelay }) "relay" else "direct"
            rememberRelay(node, store, id, server.id, relay, attempt)
            return opened
        } catch (error: Unavailable) {
            if (outcome != "timeout") outcome = error.failure.name.lowercase()
            throw error
        } catch (error: CancellationException) {
            // A cancelled switch, not an unreachable computer.
            outcome = "cancelled"
            throw error
        } catch (error: Throwable) {
            // An App Server of another protocol major does not offer this ALPN.
            val incompatible = (error.message ?: "").contains("alpn", ignoreCase = true)
            if (incompatible) outcome = "incompatible"
            throw Unavailable(if (incompatible) Failure.INCOMPATIBLE else Failure.UNREACHABLE)
        } finally {
            diagnostics.record("remote_connect", outcome, attempt, SystemClock.elapsedRealtime() - started)
        }
    }

    /**
     * Records when the connection in use stops receiving from the computer while
     * the app is on screen, which "Connecting…" logic cannot see: the connection
     * still looks open. The end record says whether this phone kept sending and
     * losing packets meanwhile, which tells a dead return path from a computer
     * that stopped answering. Healthy connections stay quiet.
     */
    // TODO: once a stall's cause is known from these records, decide whether a
    // stall should replace the endpoint as a return to the screen does.
    private fun watchReceive(opened: Connection, attempt: Int) = scope.launch {
        val watch = ReceiveWatch()
        var atStall: ConnectionStats? = null
        while (connection === opened && opened.closeReason() == null) {
            val stats = runCatching { opened.stats() }.getOrNull() ?: break
            val change = watch.sample(SystemClock.elapsedRealtime(), stats.udpRxDatagrams, hiddenAt == 0L)
            if (change is ReceiveWatch.Change.Stalled) atStall = stats
            if (change != null) recordReceive(change, attempt, opened, stats, atStall)
            delay(ReceiveWatch.SAMPLE_MS)
        }
        val ended = watch.end(SystemClock.elapsedRealtime()) ?: return@launch
        recordReceive(ended, attempt, opened, runCatching { opened.stats() }.getOrNull(), atStall)
    }

    private fun recordReceive(change: ReceiveWatch.Change, attempt: Int, opened: Connection, stats: ConnectionStats?, atStall: ConnectionStats?) {
        val (outcome, silentMs) = when (change) {
            is ReceiveWatch.Change.Stalled -> "stalled" to change.silentMs
            is ReceiveWatch.Change.Resumed -> "resumed" to change.silentMs
            is ReceiveWatch.Change.Hidden -> "hidden" to change.silentMs
            is ReceiveWatch.Change.Ended -> "ended" to change.silentMs
        }
        val fields = HashMap<String, Any>()
        val selected = runCatching { opened.paths().firstOrNull { it.isSelected } }.getOrNull()
        fields["route"] = when { selected == null -> "none"; selected.isRelay -> "relay"; else -> "direct" }
        if (selected != null) fields["rtt_ms"] = selected.rttMs.toLong()
        if (stats != null) {
            fields["rx_datagrams"] = stats.udpRxDatagrams
            fields["tx_datagrams"] = stats.udpTxDatagrams
            fields["lost_packets"] = stats.lostPackets
            if (atStall != null && change !is ReceiveWatch.Change.Stalled) {
                fields["tx_during_stall"] = stats.udpTxDatagrams - atStall.udpTxDatagrams
                fields["lost_during_stall"] = stats.lostPackets - atStall.lostPackets
            }
        }
        diagnostics.record("remote_connection_receive", outcome, attempt, silentMs, fields)
    }

    /** Keeps the relay the computer answered through, so the next connect can skip the address lookup. */
    private suspend fun rememberRelay(node: Endpoint, store: ConnectionStore, id: EndpointId, serverId: String, used: String?, attempt: Int) {
        val current = runCatching { node.remoteAddr(id)?.relayUrl() }.getOrNull()
        val outcome = when {
            !ReconnectPolicy.validRelay(current) -> "unavailable"
            current == used -> "unchanged"
            runCatching { store.saveRelay(serverId, current) }.isSuccess -> "saved"
            else -> "storage_unavailable"
        }
        diagnostics.record("remote_connect_address", outcome, attempt, 0)
    }

    /** The open connection, unless its endpoint is about to be replaced. */
    private fun live(): Connection? = connection?.takeIf { stale == null && it.closeReason() == null }

    /** Forgets a connection a stream could not be opened on; returns why it ended. */
    fun drop(failed: Connection): Failure {
        if (connection === failed) connection = null
        return failure(failed.closeReason() ?: "")
    }

    /** Ends the connection after the user forgets the computer or pairs another. */
    fun disconnect() {
        val current = connection ?: return
        connection = null
        runCatching { current.close(0L, "forgotten".toByteArray()) }
        diagnostics.record("remote_connect", "disconnected", attempts, 0)
    }

    private fun failure(closeReason: String): Failure = when {
        listOf("untrusted", "removed", "refused").any { closeReason.contains(it) } -> Failure.NOT_TRUSTED
        closeReason.contains("unknown protocol") -> Failure.INCOMPATIBLE
        else -> Failure.UNREACHABLE
    }

    /** Join direction: the App Server that trusted this phone connects to say so. */
    private suspend fun acceptAnnouncements(context: Context, node: Endpoint) {
        while (true) {
            val incoming = try { node.acceptNext() ?: break } catch (error: Throwable) { break }
            scope.launch {
                var outcome = "ignored"
                val started = SystemClock.elapsedRealtime()
                try {
                    val accepting = incoming.accept()
                    if (!accepting.alpn().contentEquals(PAIRING_ALPN)) return@launch
                    val announcing = accepting.connect()
                    try {
                        val listener = joinListener ?: return@launch
                        val stream = withTimeout(PAIRING_TIMEOUT_MS) { announcing.acceptBi() }
                        val result = JSONObject(String(withTimeout(PAIRING_TIMEOUT_MS) { stream.recv().readToEnd(PAIRING_MESSAGE_LIMIT) }))
                        if (!result.optBoolean("trusted") || joinListener !== listener) return@launch
                        joinListener = null
                        val server = ConnectionStore.PairedServer(announcing.remoteId().toString(), result.optString("serverName"))
                        ConnectionStore(context).savePaired(server)
                        // Finishing this side tells the App Server the announcement arrived.
                        stream.send().finish()
                        outcome = "trusted"
                        listener(server)
                        // Closing first could drop the acknowledgement before it is delivered.
                        withTimeoutOrNull(PAIRING_TIMEOUT_MS) { announcing.closed() }
                    } finally {
                        runCatching { announcing.close(0L, "done".toByteArray()) }
                    }
                } catch (error: Throwable) {
                    outcome = "failed"
                } finally {
                    if (outcome != "ignored") {
                        diagnostics.record("remote_pairing_join", outcome, 0, SystemClock.elapsedRealtime() - started)
                    }
                }
            }
        }
        diagnostics.record("remote_endpoint_bind", "accept_ended", 0, 0)
    }

    private const val REFUSAL_WAIT_MS = 400L
}
