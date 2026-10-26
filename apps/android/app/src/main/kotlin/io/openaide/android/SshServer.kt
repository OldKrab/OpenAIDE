package io.openaide.android

import android.content.Context
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import io.openaide.android.ssh.SshComputer
import io.openaide.android.ssh.SshEvents
import io.openaide.android.ssh.SshException
import io.openaide.android.ssh.SshLaunch
import io.openaide.android.ssh.Target
import java.io.File
import java.io.IOException
import java.net.HttpURLConnection
import java.net.URL
import java.security.SecureRandom
import java.util.concurrent.Executors
import org.json.JSONObject

/**
 * The App Server on the selected SSH Computer (ADR-0069), held by this shell as
 * [HeldServer] describes.
 *
 * The SSH library starts or attaches to the App Server there and forwards it to
 * this phone's loopback; this object chooses the computer, supplies the release
 * archive, and remembers what the last launch learned for Settings. A launch that
 * finds an App Server too old to use fails with `update_required` until the user
 * confirms [update].
 */
internal object SshServer {
    /** The first start may download the release and upload it over a phone's network. */
    private const val START_WAIT_MS = 300_000L
    private const val MAX_ARCHIVE_BYTES = 256L * 1024 * 1024
    private const val RELEASE_URL = "https://github.com/OldKrab/OpenAIDE/releases/download/v%s/openaide-app-server-linux-x64-%s.tar.gz"
    private const val BUNDLED_ARCHIVE = "ssh/openaide-app-server-linux-x64.bin"

    fun interface Connected { fun done(problem: String?, reason: String?) }

    /** What the last launch learned about the App Server there, for Settings. */
    class Status(@JvmField val runningVersion: String, @JvmField val updateAvailable: Boolean, @JvmField val suggestLinger: Boolean)

    private val diagnostics = AndroidDiagnostics()
    private val main = Handler(Looper.getMainLooper())
    private val worker = Executors.newSingleThreadExecutor { task -> Thread(task, "openaide-ssh-control").apply { isDaemon = true } }
    private val lock = Any()
    private var native: Pair<String, SshComputer>? = null
    @Volatile private var replaceNext = false
    @Volatile private var status: Status? = null
    /** The version of the App Server there when it is too old for this app; null otherwise. */
    @Volatile private var requiredUpdate: String? = null
    /** Shows the launch stage while the connection screen waits; set only by that screen. */
    @Volatile private var stageListener: ((String) -> Unit)? = null

    val held = HeldServer("ssh_server", START_WAIT_MS) { context, done -> launch(context, done) }

    @JvmStatic fun status(): Status? = status
    @JvmStatic fun requiredUpdate(): String? = requiredUpdate

    /** Opens the gateway and reaches the App Server on the selected SSH Computer before the workspace is shown. */
    @JvmStatic fun connect(context: Context, stage: ((String) -> Unit)?, callback: Connected) {
        val application = context.applicationContext
        stageListener = stage
        worker.execute {
            var reason: String? = null
            val problem = if (!WorkspaceGateway.start(application, ConnectionProfile.Kind.SSH)) {
                "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            } else try {
                held.endpoint(application)
                null
            } catch (error: HeldServer.Unavailable) {
                reason = error.reason
                message(error.reason)
            }
            stageListener = null
            main.post { callback.done(problem, reason) }
        }
    }

    /** Task status for the background service, through the forward. */
    @JvmStatic @Throws(IOException::class)
    fun status(context: Context): ServerStatus = held.status(context)

    /** Makes the next launch replace the App Server there; the user confirmed the warning. */
    @JvmStatic fun update() {
        replaceNext = true
        held.reset()
    }

    /** Forgets the computer in use, such as when the user chose another or forgot it. */
    @JvmStatic fun reset() {
        held.reset()
        val closing = synchronized(lock) { native.also { native = null } }
        status = null
        requiredUpdate = null
        replaceNext = false
        if (closing != null) worker.execute { release(closing.second) }
    }

    /**
     * An SSH Computer a switch reached (ADR-0070): its App Server started or attached on a
     * session of its own, while the computer in use keeps its session. [adopt] makes it the
     * one in use; [discard] closes it. [updateRequired] is the version there when it is too
     * old: the switch still goes ahead, and the page offers the update as a direct use does.
     */
    class Prepared internal constructor(
        val id: String,
        internal val session: SshComputer,
        internal val status: Status?,
        internal val updateRequired: String?,
    )

    /** Reaches [computer] for a switch; throws [HeldServer.Unavailable] with a failure class for [message]. Blocks. */
    @JvmStatic fun prepare(context: Context, computer: SshComputers.Computer): Prepared {
        val application = context.applicationContext
        val started = SystemClock.elapsedRealtime()
        diagnostics.record("ssh_server_prepare", "started", 1, 0)
        var outcome = "failed"
        var session: SshComputer? = null
        try {
            val opened = open(application, computer, ConnectionStore(application)).also { session = it }
            val prepared = when (val result = opened.launch(false, operation())) {
                is SshLaunch.Ready -> {
                    val ready = result.ready
                    if (HeldServer.Endpoint.of(ready.endpointUrl, ready.authToken) == null) {
                        outcome = "invalid_handoff"
                        throw HeldServer.Unavailable(outcome)
                    }
                    outcome = "ready"
                    Prepared(computer.id, opened, Status(ready.appVersion, ready.updateAvailable, ready.suggestLinger), null)
                }
                is SshLaunch.UpdateRequired -> {
                    outcome = "update_required"
                    Prepared(computer.id, opened, null, result.runningVersion)
                }
            }
            return prepared
        } catch (error: SshException.Failed) {
            outcome = error.`class`
            session?.let(::release)
            throw HeldServer.Unavailable(outcome)
        } catch (error: HeldServer.Unavailable) {
            session?.let(::release)
            throw error
        } finally {
            diagnostics.record("ssh_server_prepare", outcome, 1, SystemClock.elapsedRealtime() - started)
        }
    }

    /**
     * Makes a switch's [prepared] computer the one in use, closing the previous session.
     * The held server relaunches on the adopted session, which attaches to the App Server
     * the switch started without signing in again.
     */
    // TODO: hand the switch's endpoint to [held] instead of attaching a second time, once
    // HeldServer can take an endpoint it did not launch (it initializes and heartbeats its own).
    @JvmStatic fun adopt(prepared: Prepared) {
        held.reset()
        val closing = synchronized(lock) { native.also { native = prepared.id to prepared.session } }
        status = prepared.status
        requiredUpdate = prepared.updateRequired
        replaceNext = false
        if (closing != null && closing.second !== prepared.session) worker.execute { release(closing.second) }
    }

    /** Closes what a switch reached and no longer uses. */
    @JvmStatic fun discard(prepared: Prepared) { worker.execute { release(prepared.session) } }

    /** Ends the keeper on [computer]; reports `stopped`, `notRunning`, `stillRunning`, or a failure class. */
    @JvmStatic fun stop(context: Context, computer: SshComputers.Computer, store: ConnectionStore, done: (String) -> Unit) {
        val application = context.applicationContext
        worker.execute {
            val started = SystemClock.elapsedRealtime()
            val outcome = try {
                val selected = synchronized(lock) { native?.takeIf { it.first == computer.id }?.second }
                val stopping = selected ?: open(application, computer, store)
                try { stopping.stop(operation()) } finally { if (selected == null) release(stopping) }
            } catch (error: SshException.Failed) {
                error.`class`
            } catch (error: RuntimeException) {
                "stop_failed"
            }
            diagnostics.record("ssh_server_stop", outcome, 1, SystemClock.elapsedRealtime() - started)
            main.post { done(outcome) }
        }
    }

    private fun launch(context: Context, done: (HeldServer.Endpoint?, String) -> Unit) {
        val store = ConnectionStore(context)
        val computer = store.sshComputer() ?: return done(null, "not_selected")
        val replace = replaceNext
        replaceNext = false
        try {
            val selected = synchronized(lock) {
                native?.takeIf { it.first == computer.id }?.second ?: open(context, computer, store).also { opened ->
                    native?.second?.let(::release)
                    native = computer.id to opened
                }
            }
            when (val result = selected.launch(replace, operation())) {
                is SshLaunch.Ready -> {
                    val ready = result.ready
                    requiredUpdate = null
                    status = Status(ready.appVersion, ready.updateAvailable, ready.suggestLinger)
                    val endpoint = HeldServer.Endpoint.of(ready.endpointUrl, ready.authToken)
                    done(endpoint, if (endpoint == null) "invalid_handoff" else "ready")
                }
                is SshLaunch.UpdateRequired -> {
                    requiredUpdate = result.runningVersion
                    done(null, "update_required")
                }
            }
        } catch (error: SshException.Failed) {
            done(null, error.`class`)
        }
    }

    private fun open(context: Context, computer: SshComputers.Computer, store: ConnectionStore): SshComputer = SshComputer(
        target(computer), computer.hostKey, store.sshKey(), BuildConfig.SSH_APP_SERVER_VERSION, BuildConfig.SSH_APP_SERVER_SHA256,
        File(context.cacheDir, "ssh-app-server").absolutePath, Events(context.applicationContext))

    /** Ends the session and frees the library's object; a launch still using it fails. */
    private fun release(computer: SshComputer) {
        computer.disconnect()
        computer.close()
    }

    @JvmStatic fun target(computer: SshComputers.Computer) = Target(computer.host, computer.port.toUShort(), computer.user)

    /** A correlation id for one launch or stop; it names nothing about the computer. */
    private fun operation(): String = "android-ssh-" + ByteArray(6).also(SecureRandom()::nextBytes).joinToString("") { "%02x".format(it.toInt() and 0xff) }

    /** A sentence for a launch failure class; the class itself stays in diagnostics. */
    @JvmStatic fun message(reason: String): String = when (reason) {
        "host_key_changed" -> "This computer’s identity changed, so OpenAIDE did not connect. If you expected the change, forget the computer in Settings and add it again."
        "auth_failed", "auth_prompt_unsupported" -> "The computer did not accept this phone’s key. Add the phone’s public key to ~/.ssh/authorized_keys there, then try again."
        "host_unknown" -> "OpenAIDE could not find that computer. Check its address and your network."
        "unreachable", "connection_refused", "connection_lost", "timeout" -> "OpenAIDE could not reach the computer. Check that it is on and reachable from this phone."
        "unavailable_in_build" -> "This build of OpenAIDE does not include the App Server for SSH computers. Install a release build to use this computer."
        "download_failed", "checksum_mismatch", "archive_invalid" -> "OpenAIDE could not download itself for the computer. Check your internet connection and try again."
        "unsupported_platform" -> "OpenAIDE runs only on 64-bit x86 Linux computers."
        "update_required" -> "The computer runs an older OpenAIDE that this app cannot use."
        "server_newer" -> "The computer runs a newer OpenAIDE. Update this app to use it."
        else -> "OpenAIDE could not start on the computer. Try again."
    }

    /** The library's events for an operation outside a launch, such as adding the phone's key. */
    fun events(context: Context): SshEvents = Events(context.applicationContext)

    /** The library's lifecycle events, its stages, and the release it needs. */
    private class Events(private val context: Context) : SshEvents {
        override fun log(event: String, fields: String) {
            diagnostics.recordFields(event, fields)
        }

        override fun stage(stage: String) {
            stageListener?.let { listener -> main.post { listener(stage) } }
        }

        override fun releaseArchive(): String {
            val started = SystemClock.elapsedRealtime()
            val source = if (BuildConfig.SSH_APP_SERVER_BUNDLED) "bundled" else "download"
            diagnostics.recordFields("ssh_release_fetch_started", JSONObject().put("source", source).toString())
            val target = File(context.cacheDir, "ssh-release-${System.nanoTime()}.tar.gz")
            val outcome = try {
                if (BuildConfig.SSH_APP_SERVER_BUNDLED) context.assets.open(BUNDLED_ARCHIVE).use { input -> target.outputStream().use { input.copyTo(it) } }
                else download(target)
                "success"
            } catch (error: IOException) {
                target.delete()
                "failure"
            }
            diagnostics.recordFields("ssh_release_fetch_completed", JSONObject().put("source", source).put("outcome", outcome)
                .put("durationMs", SystemClock.elapsedRealtime() - started).toString())
            if (outcome != "success") throw SshException.Failed("fetch", "download_failed")
            return target.absolutePath
        }

        private fun download(target: File) {
            val version = BuildConfig.SSH_APP_SERVER_VERSION
            val connection = URL(RELEASE_URL.format(version, version)).openConnection() as HttpURLConnection
            try {
                connection.connectTimeout = 15_000
                connection.readTimeout = 30_000
                if (connection.responseCode != 200) throw IOException("Release unavailable")
                connection.inputStream.use { input ->
                    target.outputStream().use { output ->
                        val buffer = ByteArray(64 * 1024)
                        var total = 0L
                        while (true) {
                            val count = input.read(buffer)
                            if (count < 0) break
                            total += count
                            if (total > MAX_ARCHIVE_BYTES) throw IOException("Release too large")
                            output.write(buffer, 0, count)
                        }
                    }
                }
            } finally {
                connection.disconnect()
            }
        }
    }
}
