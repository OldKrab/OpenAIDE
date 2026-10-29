package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Test

class DialReportTest {
    private val serverId = "ab12cd34ef56" + "0".repeat(52)
    private val wifi = NetworkKind.of(wifi = true, cellular = false, ethernet = false, vpn = false, validated = true)

    @Test fun aDialThatTimedOutSaysHowFarItGotAndWhatItKnew() {
        val report = DialReport(DialReport.Purpose.SWITCH, 1, serverId, wifi)
        report.stage = DialReport.Stage.CONNECT
        report.address = "lookup"
        report.ownRelay = true
        report.knownRelay = false
        report.knownDirect = 0

        assertEquals(mapOf(
            "purpose" to "switch", "target" to "ab12cd34", "index" to 1,
            "network" to "wifi", "network_validated" to true, "vpn" to false,
            "stage" to "connect", "address" to "lookup",
            "own_relay" to "present", "known_relay" to "absent", "known_direct" to 0,
        ), report.fields(null))
    }

    @Test fun anUnexpectedFailureIsNamedByItsTypeNotItsMessage() {
        val report = DialReport(DialReport.Purpose.LIVE, 0, serverId, NetworkKind.NONE)
        val fields = report.fields(IllegalStateException("connect to 192.0.2.1:4433 failed: connection lost"))

        assertEquals("IllegalStateException", fields["error_class"])
        assertEquals("closed", fields["error_reason"])
        assertFalse(fields.values.any { it.toString().contains("192.0.2.1") })
        assertEquals("bind", fields["stage"])
    }

    @Test fun libraryErrorTextMapsToFixedWords() {
        // The text iroh reports when the computer runs another protocol major.
        assertEquals(
            "alpn",
            DialReport.errorReason(
                "aborted by peer: the cryptographic handshake failed: error 120: peer doesn't support any known protocol",
            ),
        )
        assertEquals("no_address", DialReport.errorReason("No addressing information available"))
        assertEquals("relay", DialReport.errorReason("relay connection failed"))
        assertEquals("timed_out", DialReport.errorReason("operation timed out"))
        assertEquals("other", DialReport.errorReason("something new"))
        assertNull(DialReport.errorReason(null))
    }

    @Test fun theTargetIsAShortPrefixOfThePublicId() {
        assertEquals("ab12cd34", DialReport.shortId(serverId))
        assertEquals("none", DialReport.shortId("--"))
    }

    @Test fun aVpnIsReportedBesideTheTransportUnderIt() {
        assertEquals(NetworkKind("cellular", validated = true, vpn = true),
            NetworkKind.of(wifi = false, cellular = true, ethernet = false, vpn = true, validated = true))
        assertEquals("vpn", NetworkKind.of(wifi = false, cellular = false, ethernet = false, vpn = true, validated = false).transport)
    }
}
