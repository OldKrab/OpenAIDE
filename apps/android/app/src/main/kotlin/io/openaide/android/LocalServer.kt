package io.openaide.android

import android.content.Context
import android.os.Handler
import android.os.Looper
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

/** Open tasks of the local App Server, as the background service needs them. */
class ServerStatus(@JvmField val active: Int, @JvmField val waiting: Int) {
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
 * The App Server in Termux, used as an App Shell on a computer uses its own.
 *
 * Termux attaches to the server already serving the state or launches it, and
 * hands back its loopback address and token. The shell then stays a client of
 * that server for as long as the app's process lives, so the server ends by its
 * own last-client rule when the app is gone. The token never reaches the WebView:
 * [WorkspaceGateway] adds it to each request it carries.
 */
internal object LocalServer {
    class Endpoint(val host: String, val port: Int, val path: String, val token: String)

    class Unavailable : Exception()

    fun interface Connected { fun done(problem: String?) }

    private const val CLIENT_ID = "android-app-shell"
    private const val START_WAIT_MS = 80_000L
    private const val HEARTBEAT_MS = 5_000L
    private const val MAX_HEARTBEAT_FAILURES = 3
    private const val REQUEST_TIMEOUT_MS = 10_000
    private const val INITIALIZE_TIMEOUT_MS = 60_000
    private const val MAX_RESPONSE_BYTES = 1024 * 1024
    private const val MAX_TASK_PAGES = 50

    private val diagnostics = AndroidDiagnostics()
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newScheduledThreadPool(2) { task -> Thread(task, "openaide-local-server").apply { isDaemon = true } }
    private val requests = AtomicInteger()
    private val lock = Any()
    @Volatile private var endpoint: Endpoint? = null
    private var starting: CompletableFuture<Endpoint>? = null
    private var heartbeat: ScheduledFuture<*>? = null
    private var attempts = 0

    /** The live server's endpoint, starting the server first when there is none. Blocks; never call on the main thread. */
    fun endpoint(context: Context): Endpoint {
        endpoint?.let { return it }
        val pending = synchronized(lock) {
            endpoint?.let { return it }
            starting ?: CompletableFuture<Endpoint>().also { starting = it; start(context.applicationContext, it) }
        }
        return try { pending.get(START_WAIT_MS, TimeUnit.MILLISECONDS) } catch (error: Exception) { throw Unavailable() }
    }

    /** Forgets an endpoint that stopped answering; the next request starts the server again. */
    fun invalidate(failed: Endpoint) = synchronized(lock) {
        if (endpoint !== failed) return@synchronized
        endpoint = null
        heartbeat?.cancel(false)
        heartbeat = null
        diagnostics.record("local_server", "invalidated", attempts, 0)
    }

    /** Opens the gateway and reaches the local App Server before the workspace is shown. */
    @JvmStatic fun connect(context: Context, callback: Connected) {
        val application = context.applicationContext
        worker.execute {
            val problem = if (!WorkspaceGateway.start(application, true)) {
                "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            } else try {
                endpoint(application)
                null
            } catch (error: Unavailable) {
                "Your local workspace could not start. Try again, or check this phone in Connection settings."
            }
            main.post { callback.done(problem) }
        }
    }

    /** Counts open tasks by what they need from the phone: staying awake, or the user. */
    @JvmStatic @Throws(IOException::class)
    fun status(context: Context): ServerStatus {
        val server = try { endpoint(context) } catch (error: Unavailable) { throw IOException("Local App Server unavailable") }
        var active = 0
        var waiting = 0
        var cursor: Any? = null
        val seen = HashSet<String>()
        repeat(MAX_TASK_PAGES) {
            val params = JSONObject().put("lifecycle", "open")
            if (cursor != null) params.put("cursor", cursor)
            val tasks = request(server, "task/list", params, REQUEST_TIMEOUT_MS)
            val page = tasks.optJSONArray("tasks") ?: throw IOException("Invalid task status")
            for (index in 0 until page.length()) {
                when (ServerStatus.kind(page.getJSONObject(index).optString("status")) ?: throw IOException("Unknown task status")) {
                    1 -> active++
                    0 -> waiting++
                }
            }
            cursor = tasks.opt("nextCursor")?.takeIf { it != JSONObject.NULL } ?: return ServerStatus(active, waiting)
            if (!seen.add(cursor.toString())) throw IOException("Invalid task cursor")
        }
        throw IOException("Too many task pages")
    }

    private fun start(context: Context, result: CompletableFuture<Endpoint>) {
        val attempt = ++attempts
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("local_server_start", "started", attempt, 0)
        fun finish(outcome: String, server: Endpoint?) {
            synchronized(lock) {
                starting = null
                if (server != null) {
                    endpoint = server
                    heartbeat?.cancel(false)
                    heartbeat = beat(server)
                }
            }
            diagnostics.record("local_server_start", outcome, attempt, SystemClock.elapsedRealtime() - started)
            if (server != null) result.complete(server) else result.completeExceptionally(Unavailable())
        }
        main.post {
            TermuxCommand.run(context, "start-termux.sh", TermuxCommand.variable("OPENAIDE_VERSION", BuildConfig.VERSION_NAME)) { success, output ->
                if (!success) { finish("termux_failed", null); return@run }
                worker.execute {
                    val server = parse(output)
                    if (server == null) { finish("invalid_handoff", null); return@execute }
                    try {
                        request(server, "client/initialize", JSONObject()
                            .put("clientInstanceId", CLIENT_ID)
                            .put("shell", JSONObject().put("kind", "mobile"))
                            .put("requestedSurface", JSONObject().put("kind", "home"))
                            .put("capabilities", JSONObject()
                                .put("protocol", JSONArray(listOf("requestResponses", "stableClientRequestIds", "resync")))
                                .put("shell", JSONArray())), INITIALIZE_TIMEOUT_MS)
                        finish("ready", server)
                    } catch (error: IOException) {
                        finish("initialize_failed", null)
                    }
                }
            }
        }
    }

    /** The handoff line the App Server prints: where it listens and the token it accepts. */
    internal fun parse(output: String): Endpoint? = try {
        val line = JSONObject(output.trim().lineSequence().firstOrNull() ?: "")
        val url = URL(line.getString("endpointUrl"))
        val token = line.getString("authToken")
        if (line.optString("kind") != "localHttp" || url.protocol != "http" || url.host != "127.0.0.1" || url.port <= 0
            || token.isEmpty() || token.any { it.isWhitespace() }) null
        else Endpoint(url.host, url.port, url.path.trimEnd('/'), token)
    } catch (error: Exception) { null }

    /** Keeps this shell a live client, so the server stays while the app's process does. */
    private fun beat(server: Endpoint): ScheduledFuture<*> {
        var failures = 0
        return worker.scheduleWithFixedDelay({
            try {
                request(server, "client/heartbeat", JSONObject(), REQUEST_TIMEOUT_MS)
                if (failures > 0) diagnostics.record("local_server_heartbeat", "recovered", failures, 0)
                failures = 0
            } catch (error: IOException) {
                failures++
                // Only the transition is logged; the schedule itself is the retry.
                if (failures == 1) diagnostics.record("local_server_heartbeat", "failed", failures, 0)
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
}
