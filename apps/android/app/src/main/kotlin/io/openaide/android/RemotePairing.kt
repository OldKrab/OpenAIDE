package io.openaide.android

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import com.google.zxing.BarcodeFormat
import com.google.zxing.EncodeHintType
import com.google.zxing.qrcode.QRCodeWriter
import computer.iroh.Connection
import kotlinx.coroutines.launch
import org.json.JSONObject

/**
 * What the shell's screens ask of the paired connection, with every answer
 * delivered on the main thread in words the user can act on.
 */
internal object RemotePairing {
    fun interface Paired { fun done(server: ConnectionStore.PairedServer?, problem: String?) }
    fun interface Connected { fun done(problem: String?) }
    fun interface JoinCode { fun shown(code: JSONObject?, problem: String?) }
    fun interface Checked { fun done(ok: Boolean, message: String) }

    private val main = Handler(Looper.getMainLooper())

    /** Pairs with the computer whose invite the user scanned or pasted, and selects it. */
    @JvmStatic fun redeem(context: Context, code: String, callback: Paired) {
        val invite = try { PairingCode.parseInvite(code) } catch (error: PairingCode.Invalid) {
            callback.done(null, when (error.problem) {
                PairingCode.Problem.WRONG_KIND -> "That code adds a device to a computer. On your computer, open Settings → Remote and choose Show code."
                PairingCode.Problem.MALFORMED -> "The code is incomplete. Scan or paste it again."
                PairingCode.Problem.NOT_A_PAIRING_CODE -> "That is not an OpenAIDE pairing code."
            })
            return
        }
        val application = context.applicationContext
        RemoteNode.scope.launch {
            try {
                val server = RemoteNode.redeem(application, invite, deviceName(application), deviceModel())
                ConnectionStore(application).savePaired(server)
                // A connection to a previously paired computer must not outlive the choice.
                RemoteNode.disconnect()
                main.post { callback.done(server, null) }
            } catch (error: RemoteNode.Unavailable) {
                val problem = if (error.failure == RemoteNode.Failure.NOT_TRUSTED) {
                    "This code has expired or was already used. Show a new code on your computer."
                } else message(error.failure)
                main.post { callback.done(null, problem) }
            }
        }
    }

    /** Shows this phone's own code and waits for a computer to approve it. */
    @JvmStatic fun join(context: Context, shown: JoinCode, paired: Paired) {
        val application = context.applicationContext
        RemoteNode.scope.launch {
            try {
                val code = RemoteNode.joinCode(application, deviceName(application), deviceModel())
                RemoteNode.awaitJoin { server ->
                    RemoteNode.disconnect()
                    main.post { paired.done(server, null) }
                }
                val presented = JSONObject().put("text", PairingCode.grouped(code)).put("qr", qr(code))
                main.post { shown.shown(presented, null) }
            } catch (error: RemoteNode.Unavailable) {
                main.post { shown.shown(null, message(error.failure)) }
            }
        }
    }

    @JvmStatic fun stopJoin() = RemoteNode.stopJoin()

    /** Starts the gateway and reaches the paired computer before the workspace is shown. */
    @JvmStatic fun connect(context: Context, callback: Connected) {
        val application = context.applicationContext
        RemoteNode.scope.launch {
            val problem = if (!WorkspaceGateway.start(application, ConnectionProfile.Kind.PAIRED)) {
                "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            } else try {
                RemoteNode.connection(application)
                null
            } catch (error: RemoteNode.Unavailable) { message(error.failure, selectedName(application)) }
            main.post { callback.done(problem) }
        }
    }

    /** Checks the way to the paired computer step by step and says which step failed (see [ConnectionCheck]). */
    @JvmStatic fun check(context: Context, callback: Checked) {
        val application = context.applicationContext
        RemoteNode.scope.launch {
            val server = ConnectionStore(application).pairedServer()
            val (ok, message) = if (server == null) false to "Pair with a computer first, then check again." else {
                val result = RemoteNode.check(application, server)
                (ConnectionCheck.outcome(result) == "ok") to ConnectionCheck.message(result, server.name)
            }
            main.post { callback.done(ok, message) }
        }
    }

    /**
     * The phone's steps for one page's [ComputerSwitch]: reach this phone's own App
     * Server or a paired computer's while the one in use stays live, then save the
     * choice and move the connection to it. The star keeps the iroh type out of the
     * Java signature: CI's Java 17 compiler cannot read iroh's newer class files.
     */
    @JvmStatic fun computerSwitch(context: Context, replacePage: Runnable): ComputerSwitch<*> {
        val application = context.applicationContext
        val diagnostics = AndroidDiagnostics()
        return ComputerSwitch(RemoteNode.scope, { main.post(it) }, SystemClock::elapsedRealtime,
            { event, outcome, durationMs, fields -> diagnostics.record(event, outcome, 0, durationMs, fields) },
            object : ComputerSwitch.Steps<Connection?> {
                override suspend fun connect(target: String?): Connection? {
                    if (!WorkspaceGateway.start(application, if (target == null) ConnectionProfile.Kind.LOCAL else ConnectionProfile.Kind.PAIRED)) {
                        throw ComputerSwitch.Failed("gateway", "OpenAIDE could not start its connection on this phone. Restart the app and try again.")
                    }
                    if (target == null) {
                        // Starting the App Server in Termux cannot be interrupted; a Cancel ignores its result.
                        try { LocalServer.held.endpoint(application) } catch (error: HeldServer.Unavailable) {
                            throw ComputerSwitch.Failed("local_unavailable", "Your local workspace could not start. Try again, or check this phone in Connection settings.")
                        }
                        return null
                    }
                    val server = PairedServers.find(ConnectionStore(application).pairedServers(), target)
                        ?: throw ComputerSwitch.Failed("not_paired", "This phone is no longer paired with that computer.")
                    return try { RemoteNode.reachForSwitch(application, server) } catch (error: RemoteNode.Unavailable) {
                        throw ComputerSwitch.Failed(error.failure.name.lowercase(), message(error.failure, server.name))
                    }
                }

                override fun apply(target: String?, connected: Connection?) {
                    val store = ConnectionStore(application)
                    // Leaving an SSH Computer ends its forward; its App Server keeps running there.
                    SshServer.reset()
                    if (target == null) {
                        store.selectLocal()
                        // The computer left behind keeps its work; only this phone's link to it ends.
                        RemoteNode.disconnect()
                    } else {
                        check(store.selectPaired(target))
                        RemoteNode.adopt(connected!!)
                    }
                }

                override fun discard(connected: Connection?) { connected?.let(RemoteNode::abandon) }

                override fun replacePage() = replacePage.run()
            })
    }

    /** A failure in words, naming the computer when it is known ("your computer" otherwise). */
    @JvmStatic @JvmOverloads fun message(failure: RemoteNode.Failure, name: String? = null): String {
        val computer = name?.takeIf { it.isNotBlank() }
        val subject = computer ?: "your computer"
        return when (failure) {
            RemoteNode.Failure.UNREACHABLE -> "Can’t reach $subject. Make sure it is on, OpenAIDE is open there, and both devices are online."
            RemoteNode.Failure.NOT_TRUSTED -> "${computer ?: "Your computer"} removed this phone. Pair it again in Connection settings."
            RemoteNode.Failure.INCOMPATIBLE -> "This phone and $subject run different versions of OpenAIDE. Update both, then try again."
            RemoteNode.Failure.STORAGE -> "Secure storage is unavailable. Restart OpenAIDE and try again."
        }
    }

    /** The name of the computer in use, for messages about it. */
    @JvmStatic fun selectedName(context: Context): String? = ConnectionStore(context).pairedServer()?.name

    /** The name the user gave this phone, as the computer will list it. */
    private fun deviceName(context: Context): String {
        val named = try { Settings.Global.getString(context.contentResolver, Settings.Global.DEVICE_NAME) } catch (error: RuntimeException) { null }
        return PairingCode.clampLabel(named ?: "").ifEmpty { PairingCode.clampLabel(Build.MODEL ?: "").ifEmpty { "Android device" } }
    }

    private fun deviceModel(): String = PairingCode.clampLabel(listOfNotNull(Build.MANUFACTURER, Build.MODEL).joinToString(" "))

    /** The code as dark-module rows, drawn by the setup page without an image resource. */
    private fun qr(code: String): JSONObject {
        val matrix = QRCodeWriter().encode(code, BarcodeFormat.QR_CODE, 0, 0, mapOf(EncodeHintType.MARGIN to 0))
        val path = StringBuilder()
        for (y in 0 until matrix.height) for (x in 0 until matrix.width) if (matrix[x, y]) path.append("M").append(x).append(' ').append(y).append("h1v1h-1z")
        return JSONObject().put("size", matrix.width).put("path", path.toString())
    }
}
