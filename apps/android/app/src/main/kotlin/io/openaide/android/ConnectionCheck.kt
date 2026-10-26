package io.openaide.android

/**
 * "Check the connection" in the phone's Advanced settings: the steps between this phone
 * and the paired computer, each answered on its own, so a failure names the step that
 * failed rather than "can't reach your computer". The steps run in [RemoteNode.check];
 * this file holds what they found and how it reads to the user and in the report.
 */
internal object ConnectionCheck {
    /** Whether this phone's endpoint got a home relay, through which a computer elsewhere answers. */
    enum class Relay { REACHED, NOT_REACHED, NO_ENDPOINT }

    /** How the computer answered: the connection in use, a fresh dial by route, or a failure. */
    enum class Computer { CONNECTED, RELAY, DIRECT, UNREACHABLE, NOT_TRUSTED, INCOMPATIBLE, NOT_CHECKED }

    data class Result(val network: NetworkKind, val relay: Relay, val computer: Computer)

    fun fields(result: Result): Map<String, Any> =
        result.network.fields() + mapOf("relay" to result.relay.name.lowercase(), "computer" to result.computer.name.lowercase())

    /** The outcome word of the `remote_check` record: the first step that failed, or `ok`. */
    fun outcome(result: Result): String = when {
        result.network.transport == "none" -> "offline"
        result.relay == Relay.NO_ENDPOINT -> "no_endpoint"
        result.computer in setOf(Computer.CONNECTED, Computer.RELAY, Computer.DIRECT) -> "ok"
        result.relay == Relay.NOT_REACHED -> "relay_not_reached"
        else -> result.computer.name.lowercase()
    }

    fun message(result: Result, name: String?): String {
        val computer = name?.takeIf { it.isNotBlank() } ?: "your computer"
        val on = "On ${networkLabel(result.network)}"
        return when {
            result.network.transport == "none" -> "This phone is offline. Connect to Wi-Fi or mobile data, then check again."
            result.relay == Relay.NO_ENDPOINT -> "OpenAIDE could not start its connection on this phone. Restart the app and try again."
            result.computer == Computer.CONNECTED -> "$on: connected to $computer."
            result.computer == Computer.RELAY -> "$on: reached $computer through the relay."
            result.computer == Computer.DIRECT -> "$on: reached $computer directly."
            result.computer == Computer.NOT_TRUSTED -> RemotePairing.message(RemoteNode.Failure.NOT_TRUSTED, name)
            result.computer == Computer.INCOMPATIBLE -> RemotePairing.message(RemoteNode.Failure.INCOMPATIBLE, name)
            result.relay == Relay.NOT_REACHED ->
                "$on: this phone can’t reach OpenAIDE’s relay, so $computer can’t answer from another network. " +
                    (if (result.network.vpn) "The VPN or the network may block it." else "The network may block it.")
            else -> "$on: the relay works, but $computer does not answer. Make sure it is on and OpenAIDE is open there."
        }
    }

    private fun networkLabel(network: NetworkKind): String {
        val transport = when (network.transport) {
            "wifi" -> "Wi-Fi"
            "cellular" -> "mobile data"
            "ethernet" -> "Ethernet"
            "vpn" -> "a VPN"
            else -> "this network"
        }
        return if (network.vpn && network.transport != "vpn") "$transport with a VPN" else transport
    }
}
