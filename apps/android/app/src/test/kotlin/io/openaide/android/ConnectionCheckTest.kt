package io.openaide.android

import io.openaide.android.ConnectionCheck.Computer
import io.openaide.android.ConnectionCheck.Relay
import io.openaide.android.ConnectionCheck.Result
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ConnectionCheckTest {
    private val wifi = NetworkKind("wifi", validated = true, vpn = false)
    private val cellularVpn = NetworkKind("cellular", validated = true, vpn = true)

    @Test fun eachFailingStepIsNamedInTheRecordAndTheMessage() {
        val offline = Result(NetworkKind.NONE, Relay.NOT_REACHED, Computer.UNREACHABLE)
        assertEquals("offline", ConnectionCheck.outcome(offline))
        assertTrue(ConnectionCheck.message(offline, "Desk").startsWith("This phone is offline"))

        val noEndpoint = Result(wifi, Relay.NO_ENDPOINT, Computer.NOT_CHECKED)
        assertEquals("no_endpoint", ConnectionCheck.outcome(noEndpoint))

        val blocked = Result(cellularVpn, Relay.NOT_REACHED, Computer.UNREACHABLE)
        assertEquals("relay_not_reached", ConnectionCheck.outcome(blocked))
        assertEquals(
            "On mobile data with a VPN: this phone can’t reach OpenAIDE’s relay, so Desk can’t answer from another network. The VPN or the network may block it.",
            ConnectionCheck.message(blocked, "Desk"),
        )

        val silent = Result(wifi, Relay.REACHED, Computer.UNREACHABLE)
        assertEquals("unreachable", ConnectionCheck.outcome(silent))
        assertEquals(
            "On Wi-Fi: the relay works, but your computer does not answer. Make sure it is on and OpenAIDE is open there.",
            ConnectionCheck.message(silent, null),
        )
    }

    @Test fun aComputerReachedWithoutTheRelayIsStillReached() {
        val local = Result(wifi, Relay.NOT_REACHED, Computer.DIRECT)
        assertEquals("ok", ConnectionCheck.outcome(local))
        assertEquals("On Wi-Fi: reached Desk directly.", ConnectionCheck.message(local, "Desk"))
    }

    @Test fun theRecordCarriesEveryStep() {
        assertEquals(
            mapOf("network" to "wifi", "network_validated" to true, "vpn" to false, "relay" to "reached", "computer" to "relay"),
            ConnectionCheck.fields(Result(wifi, Relay.REACHED, Computer.RELAY)),
        )
    }
}
