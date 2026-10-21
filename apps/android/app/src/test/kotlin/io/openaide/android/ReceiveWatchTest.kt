package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Test

class ReceiveWatchTest {
    private val stall = ReceiveWatch.STALL_AFTER_MS

    @Test fun aConnectionThatKeepsReceivingIsNeverReported() {
        val watch = ReceiveWatch()
        for (second in 0L..60L) assertNull(watch.sample(second * 1_000, rx = second, visible = true))
    }

    @Test fun nothingReceivedWhileOnScreenIsReportedOnceThenItsEnd() {
        val watch = ReceiveWatch()
        assertNull(watch.sample(0, rx = 10, visible = true))
        assertNull(watch.sample(stall - 1, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Stalled(stall), watch.sample(stall, rx = 10, visible = true))
        // Still stalled: already reported.
        assertNull(watch.sample(stall * 3, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Resumed(stall * 4), watch.sample(stall * 4, rx = 11, visible = true))
        assertNull(watch.sample(stall * 5, rx = 12, visible = true))
    }

    @Test fun silenceWhileHiddenIsNotAStall() {
        // Android stops a hidden app's network; the return replaces the endpoint anyway.
        val watch = ReceiveWatch()
        assertNull(watch.sample(0, rx = 10, visible = true))
        assertNull(watch.sample(stall * 2, rx = 10, visible = false))
        // Back on screen: the wait starts over from the return.
        assertNull(watch.sample(stall * 2 + 1, rx = 10, visible = true))
        assertNull(watch.sample(stall * 3, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Stalled(stall), watch.sample(stall * 3 + 1, rx = 10, visible = true))
    }

    @Test fun hidingDuringAStallEndsItsReport() {
        val watch = ReceiveWatch()
        assertNull(watch.sample(0, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Stalled(stall), watch.sample(stall, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Hidden(stall + 500), watch.sample(stall + 500, rx = 10, visible = false))
    }

    @Test fun aConnectionThatEndsDuringAStallReportsHowLongItLasted() {
        val watch = ReceiveWatch()
        assertNull(watch.sample(0, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Stalled(stall), watch.sample(stall, rx = 10, visible = true))
        assertEquals(ReceiveWatch.Change.Ended(stall * 2), watch.end(stall * 2))
        assertNull(ReceiveWatch().end(1_000))
    }
}
