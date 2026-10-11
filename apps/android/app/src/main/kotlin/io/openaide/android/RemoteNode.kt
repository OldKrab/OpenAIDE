package io.openaide.android

import android.content.Context
import android.os.SystemClock
import computer.iroh.Connection
import computer.iroh.Endpoint
import computer.iroh.EndpointAddr
import computer.iroh.EndpointId
import computer.iroh.EndpointOptions
import computer.iroh.IrohAndroid
import computer.iroh.SecretKey
import computer.iroh.presetN0
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
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
 * The endpoint lives as long as the process. The library closes every connection
 * when an endpoint is collected, and a second endpoint with the same key would
 * compete with the first for its relay.
 */
internal object RemoteNode {
    /** Why the paired computer cannot be used right now. */
    enum class Failure { UNREACHABLE, NOT_TRUSTED, INCOMPATIBLE, STORAGE }

    class Unavailable(val failure: Failure) : Exception(failure.name)

    private val APP_ALPN = "openaide/app-server/${BuildConfig.PROTOCOL_MAJOR}".toByteArray()
    private val PAIRING_ALPN = "openaide/pair/1".toByteArray()
    private const val PAIRING_MESSAGE_LIMIT = 4096u
    private const val CONNECT_TIMEOUT_MS = 15_000L
    private const val PAIRING_TIMEOUT_MS = 20_000L

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val diagnostics = AndroidDiagnostics()
    private val binding = Mutex()
    private val connecting = Mutex()
    private var endpoint: Endpoint? = null
    @Volatile private var connection: Connection? = null
    @Volatile private var joinListener: ((ConnectionStore.PairedServer) -> Unit)? = null
    private var attempts = 0

    /** Binds the endpoint with this phone's stored key, creating the key on first use. */
    suspend fun endpoint(context: Context): Endpoint = binding.withLock {
        endpoint?.let { return it }
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("remote_endpoint_bind", "started", 0, 0)
        val application = context.applicationContext
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
            diagnostics.record("remote_endpoint_bind", "ready", 0, SystemClock.elapsedRealtime() - started)
            bound
        } catch (error: Throwable) {
            diagnostics.record("remote_endpoint_bind", if (error is Unavailable) "storage_unavailable" else "failed", 0,
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
        connection?.let { if (it.closeReason() == null) return it }
        return connecting.withLock {
            connection?.let { if (it.closeReason() == null) return it }
            val server = ConnectionStore(context.applicationContext).pairedServer() ?: throw Unavailable(Failure.NOT_TRUSTED)
            val attempt = ++attempts
            val started = SystemClock.elapsedRealtime()
            diagnostics.record("remote_connect", "started", attempt, 0)
            var outcome = "unreachable"
            try {
                val node = endpoint(context)
                val opened = withTimeout(CONNECT_TIMEOUT_MS) {
                    node.connect(EndpointAddr(EndpointId.fromString(server.id), null, emptyList()), APP_ALPN)
                }
                // The App Server completes the handshake before it checks the key,
                // so a removed phone learns it from the close that follows.
                val refusal = withTimeoutOrNull(REFUSAL_WAIT_MS) { opened.closed() }
                if (refusal != null) throw Unavailable(failure(refusal))
                connection = opened
                outcome = if (opened.paths().any { it.isSelected && it.isRelay }) "relay" else "direct"
                opened
            } catch (error: Unavailable) {
                outcome = error.failure.name.lowercase()
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
    }

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
