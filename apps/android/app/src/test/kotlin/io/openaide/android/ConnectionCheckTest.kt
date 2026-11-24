package io.openaide.android

import io.openaide.android.remote.Check
import io.openaide.android.remote.CheckComputer as Computer
import io.openaide.android.remote.CheckRelay as Relay
import io.openaide.android.remote.NetworkKind
import io.openaide.android.remote.Transport
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

/** The node decides each step and the outcome; these are the words the user reads for them. */
class ConnectionCheckTest {
    private val wifi = NetworkKind(Transport.WIFI, validated = true, vpn = false)
    private val cellularVpn = NetworkKind(Transport.CELLULAR, validated = true, vpn = true)
    private val offline = NetworkKind(Transport.NONE, validated = false, vpn = false)

    private fun check(network: NetworkKind, relay: Relay, computer: Computer) = Check(network, relay, computer, "unused")

    @Test fun eachFailingStepIsNamedInTheMessage() {
        assertTrue(ConnectionCheck.message(check(offline, Relay.NOT_REACHED, Computer.UNREACHABLE), "Desk").startsWith("This phone is offline"))
        assertTrue(ConnectionCheck.message(check(wifi, Relay.NO_ENDPOINT, Computer.NOT_CHECKED), "Desk").startsWith("OpenAIDE could not start"))
        assertEquals(
            "On mobile data with a VPN: this phone can’t reach OpenAIDE’s relay, so Desk can’t answer from another network. The VPN or the network may block it.",
            ConnectionCheck.message(check(cellularVpn, Relay.NOT_REACHED, Computer.UNREACHABLE), "Desk"),
        )
        assertEquals(
            "On Wi-Fi: the relay works, but your computer does not answer. Make sure it is on and OpenAIDE is open there.",
            ConnectionCheck.message(check(wifi, Relay.REACHED, Computer.UNREACHABLE), null),
        )
    }

    @Test fun aComputerReachedWithoutTheRelayIsStillReached() {
        assertEquals("On Wi-Fi: reached Desk directly.", ConnectionCheck.message(check(wifi, Relay.NOT_REACHED, Computer.DIRECT), "Desk"))
        assertEquals("On Wi-Fi: connected to Desk.", ConnectionCheck.message(check(wifi, Relay.REACHED, Computer.CONNECTED), "Desk"))
    }
}
