package io.openaide.android

import android.content.Context
import android.os.Handler
import android.os.Looper
import java.io.IOException
import java.util.concurrent.Executors
import org.json.JSONObject

/**
 * The App Server in Termux, held by this shell as [HeldServer] describes.
 *
 * Termux attaches to the server already serving the state or launches it, and
 * hands back its loopback address and token.
 */
internal object LocalServer {
    private const val START_WAIT_MS = 80_000L

    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { task -> Thread(task, "openaide-local-connect").apply { isDaemon = true } }

    fun interface Connected { fun done(problem: String?) }

    val held = HeldServer("local_server", START_WAIT_MS) { context, done ->
        main.post {
            TermuxCommand.run(context, "start-termux.sh", TermuxCommand.variable("OPENAIDE_VERSION", BundledRuntime.expectedVersion())) { success, output ->
                if (!success) { done(null, "termux_failed"); return@run }
                val server = parse(output)
                done(server, if (server == null) "invalid_handoff" else "ready")
            }
        }
    }

    /** Opens the gateway and reaches the local App Server before the workspace is shown. */
    @JvmStatic fun connect(context: Context, callback: Connected) {
        val application = context.applicationContext
        worker.execute {
            val problem = if (!WorkspaceGateway.start(application, ConnectionProfile.Kind.LOCAL)) {
                "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            } else try {
                held.endpoint(application)
                null
            } catch (error: HeldServer.Unavailable) {
                "Your local workspace could not start. Try again, or check this phone in Connection settings."
            }
            main.post { callback.done(problem) }
        }
    }

    /** Counts open tasks by what they need from the phone: staying awake, or the user. */
    @JvmStatic @Throws(IOException::class)
    fun status(context: Context): ServerStatus = held.status(context)

    /**
     * [status] of the App Server this process already reaches, or null when there
     * is none. Never starts one: watching this phone's Tasks while a paired
     * computer is in use must not launch Termux (ADR-0070).
     */
    @JvmStatic @Throws(IOException::class)
    fun statusIfRunning(): ServerStatus? = held.statusIfRunning()

    /** The handoff line the App Server prints: where it listens and the token it accepts. */
    internal fun parse(output: String): HeldServer.Endpoint? = try {
        val line = JSONObject(output.trim().lineSequence().firstOrNull() ?: "")
        if (line.optString("kind") != "localHttp") null
        else HeldServer.Endpoint.of(line.getString("endpointUrl"), line.getString("authToken"))
    } catch (error: Exception) { null }
}
