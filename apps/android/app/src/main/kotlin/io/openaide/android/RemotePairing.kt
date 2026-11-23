package io.openaide.android

import android.content.Context
import android.os.Build
import android.os.Handler
import android.os.Looper
import android.os.SystemClock
import android.provider.Settings
import computer.iroh.Connection
import kotlinx.coroutines.launch

/**
 * What the shell's screens ask of the paired connection, with every answer
 * delivered on the main thread in words the user can act on.
 */
internal object RemotePairing {
    fun interface Paired { fun done(server: ConnectionStore.PairedServer?, problem: String?) }
    /** [reason] is the failure class in lower case, such as `unreachable`; null on success or when the gateway did not start. */
    fun interface Connected { fun done(problem: String?, reason: String?) }
    fun interface Checked { fun done(ok: Boolean, message: String) }

    private val main = Handler(Looper.getMainLooper())

    /** Pairs with the computer whose invite the user scanned or pasted, and selects it. */
    @JvmStatic fun redeem(context: Context, code: String, callback: Paired) {
        val invite = try { PairingCode.parseInvite(code) } catch (error: PairingCode.Invalid) {
            callback.done(null, when (error.problem) {
                PairingCode.Problem.FROM_OLDER_OPENAIDE -> "This code comes from an older OpenAIDE. Update OpenAIDE on that device, then make a code on the computer it should use: Settings › Remote › Add a device."
                PairingCode.Problem.MALFORMED -> "The code is incomplete. Scan or paste it again."
                PairingCode.Problem.NOT_A_PAIRING_CODE -> "This isn’t an OpenAIDE code."
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
                    "This code was already used or has expired. Make a new one on the computer."
                } else message(error.failure)
                main.post { callback.done(null, problem) }
            }
        }
    }

    /** Starts the gateway and reaches the paired computer before the workspace is shown. */
    @JvmStatic fun connect(context: Context, callback: Connected) {
        val application = context.applicationContext
        RemoteNode.scope.launch {
            var reason: String? = null
            val problem = if (!WorkspaceGateway.start(application, ConnectionProfile.Kind.PAIRED)) {
                "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            } else try {
                RemoteNode.connection(application)
                null
            } catch (error: RemoteNode.Unavailable) {
                reason = error.failure.name.lowercase()
                message(error.failure, selectedName(application))
            }
            main.post { callback.done(problem, reason) }
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
            // What a connect reached: a paired computer's connection, an SSH Computer's
            // prepared session, or nothing for this phone.
            object : ComputerSwitch.Steps<Any?> {
                override suspend fun connect(target: String?): Any? {
                    val kind = when {
                        target == null -> ConnectionProfile.Kind.LOCAL
                        ComputerSwitch.sshId(target) != null -> ConnectionProfile.Kind.SSH
                        else -> ConnectionProfile.Kind.PAIRED
                    }
                    if (!WorkspaceGateway.start(application, kind)) {
                        throw ComputerSwitch.Failed("gateway", "OpenAIDE could not start its connection on this phone. Restart the app and try again.")
                    }
                    if (target == null) {
                        // Starting the App Server in Termux cannot be interrupted; a Cancel ignores its result.
                        try { LocalServer.held.endpoint(application) } catch (error: HeldServer.Unavailable) {
                            throw ComputerSwitch.Failed("local_unavailable", "Your local workspace could not start. Try again, or check this phone in Connection settings.")
                        }
                        return null
                    }
                    val sshId = ComputerSwitch.sshId(target)
                    if (sshId != null) {
                        val computer = SshComputers.find(ConnectionStore(application).sshComputers(), sshId)
                            ?: throw ComputerSwitch.Failed("not_added", "This phone no longer has that SSH computer.")
                        // The SSH library blocks; like this phone's own start, a Cancel ignores its result.
                        return try { SshServer.prepare(application, computer) } catch (error: HeldServer.Unavailable) {
                            throw ComputerSwitch.Failed(error.reason, SshServer.message(error.reason))
                        }
                    }
                    val server = PairedServers.find(ConnectionStore(application).pairedServers(), target)
                        ?: throw ComputerSwitch.Failed("not_paired", "This phone is no longer paired with that computer.")
                    return try { RemoteNode.reachForSwitch(application, server) } catch (error: RemoteNode.Unavailable) {
                        throw ComputerSwitch.Failed(error.failure.name.lowercase(), message(error.failure, server.name))
                    }
                }

                override fun apply(target: String?, connected: Any?) {
                    val store = ConnectionStore(application)
                    if (connected is SshServer.Prepared) {
                        check(store.selectSsh(connected.id))
                        SshServer.adopt(connected)
                        // The computer left behind keeps its work; only this phone's link to it ends.
                        RemoteNode.disconnect()
                        return
                    }
                    // Leaving an SSH Computer ends its forward; its App Server keeps running there.
                    SshServer.reset()
                    if (target == null) {
                        store.selectLocal()
                        RemoteNode.disconnect()
                    } else {
                        check(store.selectPaired(target))
                        RemoteNode.adopt(connected as Connection)
                    }
                }

                override fun discard(connected: Any?) {
                    when (connected) {
                        is SshServer.Prepared -> SshServer.discard(connected)
                        is Connection -> RemoteNode.abandon(connected)
                    }
                }

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
}
