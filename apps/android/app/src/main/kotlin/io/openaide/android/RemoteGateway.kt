package io.openaide.android

import android.content.Context
import android.os.SystemClock
import computer.iroh.BiStream
import computer.iroh.Connection
import java.io.InputStream
import java.net.InetAddress
import java.net.ServerSocket
import java.net.Socket
import java.security.SecureRandom
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicInteger
import kotlinx.coroutines.runBlocking

/**
 * The paired connection's origin for the workspace WebView.
 *
 * It answers Frontend requests from the APK and carries App Server requests to
 * the paired computer, one iroh stream per request, exactly as the Web Shell's
 * proxy does on a computer. Recovery after a lost connection stays where it
 * already is: the Frontend's resumable session retries, and each retry asks
 * [RemoteNode] for a live connection.
 */
internal object RemoteGateway {
    private const val MAX_HEAD_BYTES = 16 * 1024
    private const val HEAD_TIMEOUT_MS = 30_000
    private const val CHUNK_BYTES = 64 * 1024

    /** Sent by the shell's own WebView on every request; other apps on the phone never learn it. */
    val token: String = ByteArray(32).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it.toInt() and 0xff) }

    private val workers = Executors.newCachedThreadPool { task -> Thread(task, "openaide-gateway").apply { isDaemon = true } }
    private val diagnostics = AndroidDiagnostics()
    private val requests = AtomicInteger()
    private var listener: ServerSocket? = null
    @Volatile private var failureListener: ((RemoteNode.Failure) -> Unit)? = null

    /** Reports a failure the user must act on: this phone was removed, or the versions differ. */
    fun onFailure(listener: ((RemoteNode.Failure) -> Unit)?) { failureListener = listener }

    /** Owns the loopback port for the rest of the process; false when another app holds it. */
    @Synchronized fun start(context: Context): Boolean {
        if (listener != null) return true
        val application = context.applicationContext
        val bound = try {
            ServerSocket().apply {
                reuseAddress = true
                bind(java.net.InetSocketAddress(InetAddress.getByName("127.0.0.1"), ConnectionProfile.PAIRED_PORT), 64)
            }
        } catch (error: java.io.IOException) {
            diagnostics.record("remote_gateway", "port_unavailable", 0, 0)
            return false
        }
        listener = bound
        diagnostics.record("remote_gateway", "listening", 0, 0)
        workers.execute {
            while (true) {
                val socket = try { bound.accept() } catch (error: java.io.IOException) { break }
                workers.execute { socket.use { serve(application, it) } }
            }
            diagnostics.record("remote_gateway", "stopped", 0, 0)
        }
        return true
    }

    private fun serve(context: Context, socket: Socket) {
        try {
            socket.tcpNoDelay = true
            socket.soTimeout = HEAD_TIMEOUT_MS
            val input = socket.getInputStream()
            val (text, bodyStart) = readHead(input) ?: return
            val head = GatewayHttp.parse(text)
            if (head == null) { reply(socket, 400, "Bad Request"); return }
            if (!GatewayHttp.authorized(head, token, ConnectionProfile.PAIRED_PORT)) {
                diagnostics.record("remote_gateway_request", "unauthorized", 0, 0)
                reply(socket, 403, "Forbidden")
                return
            }
            socket.soTimeout = 0
            val target = GatewayHttp.appServerTarget(head)
            if (target == null) serveFrontend(context, socket, head)
            else forward(context, socket, input, head, target, bodyStart)
        } catch (error: java.io.IOException) {
            // The WebView abandoned the request; there is no one left to answer.
        }
    }

    private fun serveFrontend(context: Context, socket: Socket, head: GatewayHttp.Head) {
        if (head.method != "GET" && head.method != "HEAD") { reply(socket, 405, "Method Not Allowed"); return }
        val route = FrontendAssets.route(head.path)
        val asset = if (route != null) "${FrontendAssets.ROOT}/index.html" else FrontendAssets.asset(head.path)
        val bytes = try {
            if (asset == null) null else context.assets.open(asset).use { it.readBytes() }
        } catch (error: java.io.IOException) { null }
        if (asset == null || bytes == null) { reply(socket, 404, "Not Found"); return }
        val body = if (route != null) FrontendAssets.withBootstrap(String(bytes, Charsets.UTF_8), route).toByteArray(Charsets.UTF_8) else bytes
        socket.getOutputStream().write(GatewayHttp.response(200, "OK", FrontendAssets.contentType(asset), body, head.method == "HEAD"))
    }

    private fun forward(context: Context, socket: Socket, input: InputStream, head: GatewayHttp.Head, target: String, bodyStart: ByteArray) {
        val request = requests.incrementAndGet()
        val started = SystemClock.elapsedRealtime()
        val stream = try { open(context) } catch (error: RemoteNode.Unavailable) {
            diagnostics.record("remote_gateway_request", error.failure.name.lowercase(), request, SystemClock.elapsedRealtime() - started)
            if (error.failure != RemoteNode.Failure.UNREACHABLE) failureListener?.invoke(error.failure)
            reply(socket, 502, "Bad Gateway")
            return
        }
        val upstream = workers.submit {
            try {
                runBlocking {
                    stream.send().writeAll(GatewayHttp.forwarded(head, target) + bodyStart)
                    val buffer = ByteArray(CHUNK_BYTES)
                    while (true) {
                        val count = input.read(buffer)
                        if (count < 0) break
                        stream.send().writeAll(buffer.copyOf(count))
                    }
                    stream.send().finish()
                }
            } catch (error: Throwable) {
                // Either side ended the request; the response side reports the outcome.
            }
        }
        var outcome = "completed"
        try {
            val output = socket.getOutputStream()
            runBlocking {
                while (true) {
                    val chunk = stream.recv().read(CHUNK_BYTES.toUInt())
                    if (chunk.isEmpty()) break
                    output.write(chunk)
                    output.flush()
                }
            }
        } catch (error: java.io.IOException) {
            outcome = "client_closed"
        } catch (error: Throwable) {
            outcome = "stream_failed"
        } finally {
            // Closing the socket ends the request reader, which may still wait on the WebView.
            runCatching { socket.close() }
            upstream.cancel(true)
            // Healthy requests stay quiet: a session polls and reconnects continuously.
            if (outcome == "stream_failed") {
                diagnostics.record("remote_gateway_request", outcome, request, SystemClock.elapsedRealtime() - started)
            }
        }
    }

    /** Opens a stream, replacing a connection that died since the last request once. */
    private fun open(context: Context): BiStream = runBlocking {
        var failure = RemoteNode.Failure.UNREACHABLE
        repeat(2) {
            val connection: Connection = RemoteNode.connection(context)
            try { return@runBlocking connection.openBi() }
            catch (error: Throwable) { failure = RemoteNode.drop(connection) }
            if (failure != RemoteNode.Failure.UNREACHABLE) throw RemoteNode.Unavailable(failure)
        }
        throw RemoteNode.Unavailable(failure)
    }

    private fun readHead(input: InputStream): Pair<String, ByteArray>? {
        val buffer = java.io.ByteArrayOutputStream(1024)
        val chunk = ByteArray(1024)
        while (buffer.size() <= MAX_HEAD_BYTES) {
            val count = input.read(chunk)
            if (count < 0) return null
            buffer.write(chunk, 0, count)
            val bytes = buffer.toByteArray()
            val end = String(bytes, Charsets.ISO_8859_1).indexOf("\r\n\r\n")
            if (end >= 0) return String(bytes, 0, end, Charsets.ISO_8859_1) to bytes.copyOfRange(end + 4, bytes.size)
        }
        return null
    }

    private fun reply(socket: Socket, status: Int, reason: String) {
        socket.getOutputStream().write(GatewayHttp.response(status, reason, "text/plain; charset=utf-8", reason.toByteArray()))
    }
}
