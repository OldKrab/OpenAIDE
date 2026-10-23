package io.openaide.android

import android.content.Context
import android.os.SystemClock
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.util.concurrent.CompletableFuture
import java.util.concurrent.Executors
import java.util.concurrent.ScheduledFuture
import java.util.concurrent.TimeUnit
import java.util.concurrent.atomic.AtomicInteger
import org.json.JSONArray
import org.json.JSONObject

/** Open tasks of the App Server in use, as the background service needs them. */
internal class ServerStatus(
    @JvmField val active: Int,
    @JvmField val waiting: Int,
    /** The current Task Attention Event of each open task that has one. */
    @JvmField val attention: List<AttentionNotifications.Attention>,
) {
    companion object {
        /** 1 for a task that is working, 0 for one waiting on the user, -1 for one at rest, null when unknown. */
        @JvmStatic fun kind(status: String): Int? = when (status) {
            "preparing", "starting", "running", "background", "stopping" -> 1
            "waiting" -> 0
            "idle", "interrupted", "failed", "completed" -> -1
            else -> null
        }
    }
}

/**
 * An App Server this shell holds as a client, as an App Shell on a computer holds its own.
 *
 * The first request starts or attaches to the server through [Launch], which hands back
 * a loopback address and token. The shell then initializes as the mobile client and
 * heartbeats, so the server keeps it counted for as long as the app's process lives and
 * ends by its own last-client rule once the app is gone. A server that stops answering
 * is forgotten, and the next request launches again. The token never reaches the
 * WebView: [WorkspaceGateway] adds it to each request it carries.
 *
 * [LocalServer] holds the App Server in Termux and [SshServer] the one on an SSH
 * Computer; only how the server is reached differs.
 */
internal class HeldServer(
    /** Names this server's diagnostics, such as `local_server`. */
    private val name: String,
    private val startWaitMs: Long,
    private val launch: Launch,
) {
    class Endpoint(val host: String, val port: Int, val path: String, val token: String) {
        companion object {
            /** A loopback App Server address and its token, or null for anything else. */
            fun of(endpointUrl: String, token: String): Endpoint? = try {
                val url = URL(endpointUrl)
                if (url.protocol != "http" || url.host != "127.0.0.1" || url.port <= 0 || token.isEmpty() || token.any { it.isWhitespace() }) null
                else Endpoint(url.host, url.port, url.path.trimEnd('/'), token)
            } catch (error: Exception) { null }
        }
    }

    /** No server could be reached; [reason] is the last launch's stable outcome, such as `auth_failed`. */
    class Unavailable(val reason: String) : Exception()

    /** Starts or attaches to the server. Runs on a worker thread; calls [done] exactly once. */
    fun interface Launch {
        fun start(context: Context, done: (endpoint: Endpoint?, outcome: String) -> Unit)
    }

    private val diagnostics = AndroidDiagnostics()
    private val worker = Executors.newScheduledThreadPool(2) { task -> Thread(task, "openaide-$name").apply { isDaemon = true } }
    private val requests = AtomicInteger()
    private val lock = Any()
    @Volatile private var endpoint: Endpoint? = null
    private var starting: CompletableFuture<Endpoint>? = null
    private var heartbeat: ScheduledFuture<*>? = null
    private var attempts = 0
    /** Bumped by [reset], so a launch begun for an earlier choice cannot become the endpoint. */
    private var generation = 0

    /** The live server's endpoint, launching first when there is none. Blocks; never call on the main thread. */
    fun endpoint(context: Context): Endpoint {
        endpoint?.let { return it }
        val pending = synchronized(lock) {
            endpoint?.let { return it }
            starting ?: CompletableFuture<Endpoint>().also { starting = it; start(context.applicationContext, it) }
        }
        return try { pending.get(startWaitMs, TimeUnit.MILLISECONDS) }
        catch (error: java.util.concurrent.ExecutionException) { throw error.cause as? Unavailable ?: Unavailable("launch_failed") }
        catch (error: java.util.concurrent.TimeoutException) { throw Unavailable("timeout") }
        catch (error: InterruptedException) { throw Unavailable("interrupted") }
    }

    /** Forgets an endpoint that stopped answering; the next request launches again. */
    fun invalidate(failed: Endpoint) = synchronized(lock) {
        if (endpoint !== failed) return@synchronized
        forget()
        diagnostics.record(name, "invalidated", attempts, 0)
    }

    /** Forgets the server and any launch in flight, such as after the user chose another one. */
    fun reset() = synchronized(lock) {
        generation++
        forget()
        starting?.completeExceptionally(Unavailable("reset"))
        starting = null
    }

    private fun forget() {
        endpoint = null
        heartbeat?.cancel(false)
        heartbeat = null
    }

    /** Counts open tasks by what they need from the phone: staying awake, or the user. */
    @Throws(IOException::class)
    fun status(context: Context): ServerStatus {
        val server = try { endpoint(context) } catch (error: Unavailable) { throw IOException("App Server unavailable") }
        return status(server)
    }

    /** [status] of the server already reached, or null when there is none; never starts one. */
    @Throws(IOException::class)
    fun statusIfRunning(): ServerStatus? = endpoint?.let(::status)

    private fun status(server: Endpoint): ServerStatus {
        var active = 0
        var waiting = 0
        var cursor: Any? = null
        val seen = HashSet<String>()
        val attention = ArrayList<AttentionNotifications.Attention>()
        repeat(MAX_TASK_PAGES) {
            val params = JSONObject().put("lifecycle", "open")
            if (cursor != null) params.put("cursor", cursor)
            val tasks = request(server, "task/list", params, REQUEST_TIMEOUT_MS)
            val page = tasks.optJSONArray("tasks") ?: throw IOException("Invalid task status")
            for (index in 0 until page.length()) {
                val task = page.getJSONObject(index)
                when (ServerStatus.kind(task.optString("status")) ?: throw IOException("Unknown task status")) {
                    1 -> active++
                    0 -> waiting++
                }
                AttentionWire.attention(task)?.let(attention::add)
            }
            cursor = tasks.opt("nextCursor")?.takeIf { it != JSONObject.NULL } ?: return ServerStatus(active, waiting, attention)
            if (!seen.add(cursor.toString())) throw IOException("Invalid task cursor")
        }
        throw IOException("Too many task pages")
    }

    private fun start(context: Context, result: CompletableFuture<Endpoint>) {
        val attempt = ++attempts
        val launched = generation
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("${name}_start", "started", attempt, 0)
        fun finish(outcome: String, server: Endpoint?) {
            val current = synchronized(lock) {
                val current = launched == generation
                if (current) starting = null
                if (current && server != null) {
                    endpoint = server
                    heartbeat?.cancel(false)
                    heartbeat = beat(server)
                }
                current
            }
            val recorded = if (current) outcome else "superseded"
            diagnostics.record("${name}_start", recorded, attempt, SystemClock.elapsedRealtime() - started)
            if (server != null && current) result.complete(server) else result.completeExceptionally(Unavailable(outcome))
        }
        worker.execute {
            try {
                launch.start(context) { server, outcome ->
                    if (server == null) { finish(outcome, null); return@start }
                    worker.execute {
                        try {
                            initialize(server)
                            finish("ready", server)
                        } catch (error: IOException) {
                            finish("initialize_failed", null)
                        }
                    }
                }
            } catch (error: RuntimeException) {
                finish("launch_failed", null)
            }
        }
    }

    private fun initialize(server: Endpoint) {
        request(server, "client/initialize", AttentionWire.initializeParams(CLIENT_ID), INITIALIZE_TIMEOUT_MS)
    }

    /** Keeps this shell a live client, so the server stays while the app's process does. */
    private fun beat(server: Endpoint): ScheduledFuture<*> {
        var failures = 0
        return worker.scheduleWithFixedDelay({
            try {
                request(server, "client/heartbeat", JSONObject(), REQUEST_TIMEOUT_MS)
                if (failures > 0) diagnostics.record("${name}_heartbeat", "recovered", failures, 0)
                failures = 0
            } catch (error: IOException) {
                failures++
                // Only the transition is logged; the schedule itself is the retry.
                if (failures == 1) diagnostics.record("${name}_heartbeat", "failed", failures, 0)
                if (failures >= MAX_HEARTBEAT_FAILURES) invalidate(server)
            }
        }, HEARTBEAT_MS, HEARTBEAT_MS, TimeUnit.MILLISECONDS)
    }

    private fun request(server: Endpoint, method: String, params: JSONObject, timeoutMs: Int): JSONObject {
        val id = "android-shell-${requests.incrementAndGet()}"
        val connection = URL("http", server.host, server.port, server.path.ifEmpty { "/" }).openConnection() as HttpURLConnection
        try {
            connection.connectTimeout = 2_000
            connection.readTimeout = timeoutMs
            connection.requestMethod = "POST"
            connection.doOutput = true
            connection.setRequestProperty("Authorization", "Bearer ${server.token}")
            connection.setRequestProperty("Content-Type", "application/json")
            connection.setRequestProperty("X-OpenAIDE-Connection-Id", CLIENT_ID)
            connection.outputStream.use {
                it.write(JSONObject().put("jsonrpc", "2.0").put("id", id).put("method", method).put("params", params).toString().toByteArray())
            }
            if (connection.responseCode != 200) throw IOException("App Server refused the shell request")
            val body = connection.inputStream.use { input ->
                val buffer = java.io.ByteArrayOutputStream()
                val chunk = ByteArray(8192)
                while (true) {
                    val count = input.read(chunk)
                    if (count < 0) break
                    buffer.write(chunk, 0, count)
                    if (buffer.size() > MAX_RESPONSE_BYTES) throw IOException("App Server response too large")
                }
                buffer.toString("UTF-8").trim()
            }
            val message = if (body.startsWith("[")) {
                val messages = JSONArray(body)
                (0 until messages.length()).map(messages::getJSONObject).firstOrNull { it.optString("id") == id }
            } else JSONObject(body)
            if (message == null || message.has("error")) throw IOException("App Server shell request failed")
            // A protocol result arrives wrapped in the envelope's own result.
            val result = message.optJSONObject("result") ?: return JSONObject()
            return result.optJSONObject("result") ?: result
        } catch (error: org.json.JSONException) {
            throw IOException("Invalid App Server response")
        } finally {
            connection.disconnect()
        }
    }

    private companion object {
        const val CLIENT_ID = "android-app-shell"
        const val HEARTBEAT_MS = 5_000L
        const val MAX_HEARTBEAT_FAILURES = 3
        const val REQUEST_TIMEOUT_MS = 10_000
        const val INITIALIZE_TIMEOUT_MS = 60_000
        const val MAX_RESPONSE_BYTES = 1024 * 1024
        const val MAX_TASK_PAGES = 50
    }
}
