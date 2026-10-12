package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class ReconnectPolicyTest {
    @Test fun onlyAnAppHiddenLongEnoughStartsOver() {
        assertFalse(ReconnectPolicy.staleAfterHidden(0))
        assertFalse(ReconnectPolicy.staleAfterHidden(ReconnectPolicy.STALE_AFTER_HIDDEN_MS))
        assertTrue(ReconnectPolicy.staleAfterHidden(ReconnectPolicy.STALE_AFTER_HIDDEN_MS + 1))
    }

    @Test fun attemptsGrowAndStayWithinTheBudget() {
        assertEquals(4_000L, ReconnectPolicy.attemptTimeoutMs(0, 0))
        assertEquals(8_000L, ReconnectPolicy.attemptTimeoutMs(1, 4_000))
        assertEquals(12_000L, ReconnectPolicy.attemptTimeoutMs(2, 12_000))
        assertEquals(0L, ReconnectPolicy.attemptTimeoutMs(3, 0))
        assertEquals(0L, ReconnectPolicy.attemptTimeoutMs(-1, 0))
    }

    @Test fun anAttemptThatFailedEarlyLeavesItsTimeToTheNext() {
        // An endpoint replaced under a dial fails it at once; the next attempt is not shortened.
        assertEquals(8_000L, ReconnectPolicy.attemptTimeoutMs(1, 50))
    }

    @Test fun theLastAttemptEndsWithTheBudget() {
        assertEquals(5_000L, ReconnectPolicy.attemptTimeoutMs(2, ReconnectPolicy.CONNECT_BUDGET_MS - 5_000))
        assertEquals(0L, ReconnectPolicy.attemptTimeoutMs(2, ReconnectPolicy.CONNECT_BUDGET_MS - 999))
        assertEquals(0L, ReconnectPolicy.attemptTimeoutMs(1, ReconnectPolicy.CONNECT_BUDGET_MS + 1))
    }

    @Test fun onlyTheFirstAttemptUsesTheRememberedRelay() {
        val relay = "https://relay.example./"
        assertEquals(relay, ReconnectPolicy.relayFor(0, relay))
        assertNull(ReconnectPolicy.relayFor(1, relay))
        assertNull(ReconnectPolicy.relayFor(0, null))
    }

    @Test fun aStoredRelayThatIsNotAnAddressIsIgnored() {
        assertNull(ReconnectPolicy.relayFor(0, ""))
        assertNull(ReconnectPolicy.relayFor(0, "http://relay.example./"))
        assertNull(ReconnectPolicy.relayFor(0, "https://relay.example./\nnext"))
        assertNull(ReconnectPolicy.relayFor(0, "https://" + "a".repeat(200)))
    }
}
