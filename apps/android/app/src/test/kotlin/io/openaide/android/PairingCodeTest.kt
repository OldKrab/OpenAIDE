package io.openaide.android

import org.junit.Assert.assertArrayEquals
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertThrows
import org.junit.Test

class PairingCodeTest {
    // Encoded by the App Server's own tests (`pairing_code_tests.rs`): a device
    // named "Tablet" with no model.
    private val serverJoinFixture = "OAJ15VESRRRI2HBMN2XJAM4JAWMVMEUVSJZ2LRR7SNRWYFDBJLEHG7IQMVDBMJWGK5AA"

    @Test fun joinCodeMatchesTheAppServerEncoding() {
        val payload = PairingCode.decodeBase32(serverJoinFixture.substring(4))!!
        val device = payload.copyOfRange(0, 32)
        assertEquals(serverJoinFixture, PairingCode.join(device, " Tablet ", null))
        assertEquals("OAJ1 5VES RRRI", PairingCode.grouped(serverJoinFixture).substring(0, 14))
    }

    @Test fun inviteToleratesPastedFormatting() {
        val server = ByteArray(32) { 1 }
        val secret = ByteArray(16) { 7 }
        val code = "OAI1" + PairingCode.encodeBase32(server + secret)
        val pasted = " ${code.substring(0, 10).lowercase()}-${code.substring(10).chunked(4).joinToString(" ")} \n"
        val invite = PairingCode.parseInvite(pasted)
        assertArrayEquals(server, invite.server)
        assertArrayEquals(secret, invite.secret)
        assertEquals("01".repeat(32), invite.serverId)
        assertEquals(PairingCode.encodeBase32(secret), invite.secretText)
    }

    @Test fun namesWhatIsWrongWithACode() {
        fun problem(code: String) = assertThrows(PairingCode.Invalid::class.java) { PairingCode.parseInvite(code) }.problem
        assertEquals(PairingCode.Problem.WRONG_KIND, problem(serverJoinFixture))
        assertEquals(PairingCode.Problem.NOT_A_PAIRING_CODE, problem("https://example.com"))
        assertEquals(PairingCode.Problem.MALFORMED, problem("OAI1!!!"))
        assertEquals(PairingCode.Problem.MALFORMED, problem("OAI1" + PairingCode.encodeBase32(ByteArray(40))))
    }

    @Test fun base32RoundTripsEveryLengthAndRejectsStrayBits() {
        for (length in 0..40) {
            val bytes = ByteArray(length) { (it * 37 + length).toByte() }
            assertArrayEquals(bytes, PairingCode.decodeBase32(PairingCode.encodeBase32(bytes)))
        }
        assertEquals("MZXW6YTBOI", PairingCode.encodeBase32("foobar".toByteArray()))
        assertNull(PairingCode.decodeBase32("A"))
        assertNull(PairingCode.decodeBase32("MZXW6YTBOJ"))
        assertNull(PairingCode.decodeBase32("mzxw6"))
    }

    @Test fun labelsAreClampedOnACharacterBoundaryWithoutControlCharacters() {
        val clamped = PairingCode.clampLabel("é".repeat(40))
        assertEquals(PairingCode.MAX_LABEL_BYTES, clamped.toByteArray().size)
        assertEquals("ab", PairingCode.clampLabel("a\u0007b"))
        assertEquals("", PairingCode.clampLabel("   "))
    }
}
