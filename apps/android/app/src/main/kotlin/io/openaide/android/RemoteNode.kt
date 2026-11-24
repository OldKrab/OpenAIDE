package io.openaide.android

import android.content.Context
import android.net.ConnectivityManager
import android.net.NetworkCapabilities
import android.os.SystemClock
import io.openaide.android.remote.Check
import io.openaide.android.remote.Invite
import io.openaide.android.remote.NetworkKind
import io.openaide.android.remote.Reached
import io.openaide.android.remote.Remote
import io.openaide.android.remote.RemoteException
import io.openaide.android.remote.RemoteHost
import io.openaide.android.remote.StorageException
import io.openaide.android.remote.Stream
import io.openaide.android.remote.Transport
import io.openaide.android.remote.networkKind
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob

/**
 * This phone as a Remote Device (ADR-0062): the shell's side of the node in
 * `openaide-rs/android-remote`, which owns the endpoint, pairing, the connection to
 * the computer in use, and the rules that keep that connection on a phone.
 *
 * The shell keeps what only it can: the key and paired computers in [ConnectionStore],
 * the network Android reports, the app leaving and returning to the screen, and the
 * diagnostics log the node's metadata-only events are written to. The node is made
 * on first use, so a phone that never pairs never loads its library.
 */
internal object RemoteNode {
    /** Why the paired computer cannot be used right now. */
    enum class Failure { UNREACHABLE, NOT_TRUSTED, INCOMPATIBLE, STORAGE }

    class Unavailable(val failure: Failure) : Exception(failure.name)

    val scope = CoroutineScope(SupervisorJob() + Dispatchers.IO)
    private val diagnostics = AndroidDiagnostics()
    private var created: Remote? = null
    private var hidden = false

    /** The app left the screen; Android may stop its network from here on. */
    @Synchronized fun hidden() {
        hidden = true
        created?.hidden()
    }

    /** The app is on screen again; after a longer absence the node replaces its endpoint. */
    @Synchronized fun shown() {
        hidden = false
        created?.shown()
    }

    /** Spends an invite: connects to the App Server it names and presents its secret. */
    suspend fun redeem(context: Context, invite: Invite, name: String, model: String?): ConnectionStore.PairedServer {
        val paired = call { remote(context).redeem(invite, name, model) }
        return ConnectionStore.PairedServer(paired.id, paired.name)
    }

    /** Reaches the computer in use, unless the connection to it is live. */
    suspend fun connect(context: Context) = call { remote(context).connect() }

    /** Opens the stream for one request to the computer in use; the caller closes it. */
    suspend fun open(context: Context): Stream = call { remote(context).open() }

    /**
     * Reaches [server] for a switch while the connection in use stays live: the
     * result is not used until [adopt] takes it, and a failed attempt keeps the
     * endpoint that connection rides on.
     */
    suspend fun reachForSwitch(context: Context, server: ConnectionStore.PairedServer): Reached =
        call { remote(context).reachForSwitch(server.id) }

    /** Makes a connection reached for a switch the one in use, ending the previous one. */
    fun adopt(context: Context, reached: Reached) {
        remote(context).adopt(reached)
        reached.close()
    }

    /** Ends a connection reached for a switch that was cancelled or could not be saved. */
    fun abandon(context: Context, reached: Reached) {
        remote(context).abandon(reached)
        reached.close()
    }

    /** Runs the steps of [ConnectionCheck]; recorded by the node as one `remote_check` pair. */
    suspend fun check(context: Context, server: ConnectionStore.PairedServer): Check =
        call { remote(context).check(server.id) }

    /** Ends the connection after the user forgets the computer or uses another. */
    @Synchronized fun disconnect() {
        created?.disconnect()
    }

    /** The node, made with this app's key and storage on first use. */
    @Synchronized private fun remote(context: Context): Remote {
        created?.let { return it }
        val application = context.applicationContext
        val started = SystemClock.elapsedRealtime()
        return try {
            RemoteLibrary.load(application)
            Remote(AndroidRemoteHost(application)).also {
                // The node starts after the app may already have left the screen.
                if (hidden) it.hidden()
                created = it
                diagnostics.record("remote_library_load", "ready", 0, SystemClock.elapsedRealtime() - started)
            }
        } catch (error: Throwable) {
            // A library missing from this build, or a native failure: no remote work is possible.
            diagnostics.record("remote_library_load", "failed", 0, SystemClock.elapsedRealtime() - started,
                mapOf("error_class" to error.javaClass.simpleName))
            throw Unavailable(Failure.UNREACHABLE)
        }
    }

    /** One call into the node, its failure as a [Failure]. */
    private suspend fun <T> call(block: suspend () -> T): T = try {
        block()
    } catch (error: RemoteException) {
        throw Unavailable(when (error) {
            is RemoteException.Unreachable -> Failure.UNREACHABLE
            is RemoteException.NotTrusted -> Failure.NOT_TRUSTED
            is RemoteException.Incompatible -> Failure.INCOMPATIBLE
            is RemoteException.Storage -> Failure.STORAGE
        })
    } catch (error: Unavailable) {
        throw error
    } catch (error: CancellationException) {
        throw error
    } catch (error: Exception) {
        // A panic in the library arrives as an internal error; the screens treat it as
        // an unreachable computer rather than ending the app.
        diagnostics.record("remote_library_call", "failed", 0, 0, mapOf("error_class" to error.javaClass.simpleName))
        throw Unavailable(Failure.UNREACHABLE)
    }

    /** The transport this phone dials over; never an address or a network name. */
    private fun network(context: Context): NetworkKind = runCatching {
        val manager = context.getSystemService(ConnectivityManager::class.java) ?: return UNKNOWN_NETWORK
        val active = manager.activeNetwork ?: return NetworkKind(Transport.NONE, validated = false, vpn = false)
        val capabilities = manager.getNetworkCapabilities(active) ?: return UNKNOWN_NETWORK
        networkKind(
            wifi = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_WIFI),
            cellular = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_CELLULAR),
            ethernet = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_ETHERNET),
            vpn = capabilities.hasTransport(NetworkCapabilities.TRANSPORT_VPN),
            validated = capabilities.hasCapability(NetworkCapabilities.NET_CAPABILITY_VALIDATED),
        )
    }.getOrDefault(UNKNOWN_NETWORK)

    private val UNKNOWN_NETWORK = NetworkKind(Transport.UNKNOWN, validated = false, vpn = false)

    /**
     * What the node keeps in and learns from this app. Called on the library's threads;
     * a call that is not declared to fail never throws, or the library would stop.
     */
    private class AndroidRemoteHost(private val context: Context) : RemoteHost {
        private val diagnostics = AndroidDiagnostics()

        override fun deviceKey(): ByteArray? = stored { ConnectionStore(context).deviceKey() }

        override fun saveDeviceKey(key: ByteArray) = stored { ConnectionStore(context).saveDeviceKey(key) }

        /** The paired computer chosen for remote work, which the workspace and attention use. */
        override fun computerInUse(): String? = runCatching { ConnectionStore(context).pairedServer()?.id }.getOrNull()

        override fun relay(computer: String): String? = runCatching { ConnectionStore(context).relay(computer) }.getOrNull()

        override fun saveRelay(computer: String, relay: String) = stored { ConnectionStore(context).saveRelay(computer, relay) }

        override fun network(): NetworkKind = RemoteNode.network(context)

        override fun log(event: String, fields: String) {
            runCatching { diagnostics.recordFields(event, fields) }
        }

        /** Android Keystore failures, as the one storage failure the node understands. */
        private inline fun <T> stored(block: () -> T): T = try {
            block()
        } catch (error: RuntimeException) {
            throw StorageException.Unavailable()
        }
    }
}

/** Loads the node's library and gives it this app's context for DNS. */
internal object RemoteLibrary {
    @Volatile private var loaded = false

    @Synchronized fun load(context: Context) {
        if (loaded) return
        System.loadLibrary("openaide_android_remote")
        // The resolver reads Android's nameservers through the app context.
        installContext(context.applicationContext)
        loaded = true
    }

    @JvmStatic private external fun installContext(context: Context)
}
