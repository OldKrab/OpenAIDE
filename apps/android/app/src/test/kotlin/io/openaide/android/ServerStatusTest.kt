package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ServerStatusTest {
    @Test fun sortsTasksByWhatThePhoneOwesThem() {
        for (status in listOf("preparing", "starting", "running", "background", "stopping")) assertEquals(1, ServerStatus.kind(status))
        assertEquals(0, ServerStatus.kind("waiting"))
        for (status in listOf("idle", "interrupted", "failed", "completed")) assertEquals(-1, ServerStatus.kind(status))
        // A status this build does not know must not be counted as idle and release the wake lock.
        assertNull(ServerStatus.kind("paused"))
        assertNull(ServerStatus.kind(""))
    }
}
