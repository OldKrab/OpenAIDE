package io.openaide.android

import io.openaide.android.remote.Check
import io.openaide.android.remote.CheckComputer
import io.openaide.android.remote.CheckRelay
import io.openaide.android.remote.NetworkKind
import io.openaide.android.remote.Transport

/**
 * "Check the connection" in the phone's Advanced settings: the steps between this phone
 * and the paired computer, each answered on its own, so a failure names the step that
 * failed rather than "can't reach your computer". The node runs the steps and records
 * them; this file says what they found in words the user can act on.
 */
internal object ConnectionCheck {
    fun message(result: Check, name: String?): String {
        val computer = name?.takeIf { it.isNotBlank() } ?: "your computer"
        val on = "On ${networkLabel(result.network)}"
        return when {
            result.network.transport == Transport.NONE -> "This phone is offline. Connect to Wi-Fi or mobile data, then check again."
            result.relay == CheckRelay.NO_ENDPOINT -> "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            result.computer == CheckComputer.CONNECTED -> "$on: connected to $computer."
            result.computer == CheckComputer.RELAY -> "$on: reached $computer through the relay."
            result.computer == CheckComputer.DIRECT -> "$on: reached $computer directly."
            result.computer == CheckComputer.NOT_TRUSTED -> RemotePairing.message(RemoteNode.Failure.NOT_TRUSTED, name)
            result.computer == CheckComputer.INCOMPATIBLE -> RemotePairing.message(RemoteNode.Failure.INCOMPATIBLE, name)
            result.relay == CheckRelay.NOT_REACHED ->
                "$on: this phone can’t reach OpenAIDE’s relay, so $computer can’t answer from another network. " +
                    (if (result.network.vpn) "The VPN or the network may block it." else "The network may block it.")
            else -> "$on: the relay works, but $computer does not answer. Make sure it is on and OpenAIDE is open there."
        }
    }

    private fun networkLabel(network: NetworkKind): String {
        val transport = when (network.transport) {
            Transport.WIFI -> "Wi-Fi"
            Transport.CELLULAR -> "mobile data"
            Transport.ETHERNET -> "Ethernet"
            Transport.VPN -> "a VPN"
            else -> "this network"
        }
        return if (network.vpn && network.transport != Transport.VPN) "$transport with a VPN" else transport
    }
}
