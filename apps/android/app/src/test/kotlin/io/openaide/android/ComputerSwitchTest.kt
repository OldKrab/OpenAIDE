package io.openaide.android

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerSwitchTest {
    /** The phone as the switch sees it: what is selected, what is live, and what happened in which order. */
    private class Phone {
        var selected: String? = null
        var live: String? = "connection-to-phone"
        val events: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf())
        val answer = CompletableDeferred<Result<String>>()
        val connected = CompletableDeferred<Unit>()
        val mainQueue = java.util.concurrent.LinkedBlockingQueue<() -> Unit>()
        val logged = mutableListOf<Pair<String, Map<String, String>>>()

        val steps = object : ComputerSwitch.Steps<String> {
            override suspend fun connect(target: String?): String {
                events += "connect $target"
                connected.complete(Unit)
                return answer.await().getOrThrow()
            }
            override fun apply(target: String?, connected: String) {
                events += "apply $target"
                selected = target
                live = connected
            }
            override fun discard(connected: String) { events += "discard $connected" }
            override fun replacePage() { events += "replace page" }
        }

        val switch = ComputerSwitch(
            CoroutineScope(SupervisorJob() + Dispatchers.Default),
            { mainQueue.put(it) },
            { 0L },
            { _, outcome, _, fields -> synchronized(logged) { logged += outcome to fields } },
            steps,
        )

        fun reply(): ComputerSwitch.Reply = ComputerSwitch.Reply { outcome, problem -> events += "reply ${outcome ?: problem}" }

        /** Runs what the switch posted to the main thread, waiting for the connect's result to arrive. */
        fun runMain(count: Int) = repeat(count) {
            mainQueue.poll(5, java.util.concurrent.TimeUnit.SECONDS)!!.invoke()
        }
    }

    @Test fun aConnectedTargetIsSavedAnsweredAndOnlyThenReplacesThePage() {
        val phone = Phone()
        phone.switch.request(null, "studio", "computer-switch-1", phone.reply())
        assertTrue(phone.switch.switching)
        phone.answer.complete(Result.success("connection-to-studio"))
        phone.runMain(2)

        assertEquals(listOf("connect studio", "apply studio", "reply switched", "replace page"), phone.events)
        assertEquals("studio", phone.selected)
        assertEquals("connection-to-studio", phone.live)
        assertFalse(phone.switch.switching)
        assertEquals(listOf("started", "switched"), phone.logged.map { it.first })
        assertEquals(
            mapOf("operation_id" to "computer-switch-1", "source_kind" to "this_device", "target_kind" to "paired"),
            phone.logged.last().second,
        )
    }

    @Test fun anUnreachableTargetChangesNothingAndSaysWhy() {
        val phone = Phone()
        phone.switch.request(null, "studio", null, phone.reply())
        phone.answer.complete(Result.failure(ComputerSwitch.Failed("unreachable", "Your computer is unreachable.")))
        phone.runMain(1)

        assertEquals(listOf("connect studio", "reply Your computer is unreachable."), phone.events)
        assertEquals(null, phone.selected)
        assertEquals("connection-to-phone", phone.live)
        assertEquals("unreachable", phone.logged.last().second["error_class"])
        assertTrue(phone.mainQueue.isEmpty())
    }

    @Test fun cancellingAnswersCancelledAndDropsWhatArrivesLater() {
        val phone = Phone()
        phone.switch.request("studio", null, null, phone.reply())
        runBlocking { withTimeout(5_000) { phone.connected.await() } }
        assertTrue(phone.switch.cancel())
        assertFalse(phone.switch.cancel())

        assertEquals(listOf("connect null", "reply cancelled"), phone.events)
        assertEquals("connection-to-phone", phone.live)
        assertEquals(listOf("started", "cancelled"), phone.logged.map { it.first })
    }

    @Test fun aTargetReachedAfterCancelIsGivenUp() {
        val phone = Phone()
        val gate = CompletableDeferred<Unit>()
        val switch = ComputerSwitch(
            CoroutineScope(SupervisorJob() + Dispatchers.Default),
            { phone.mainQueue.put(it) },
            { 0L },
            { _, _, _, _ -> },
            object : ComputerSwitch.Steps<String> by phone.steps {
                // A blocking start, as this phone's own App Server has, ignores the Cancel.
                override suspend fun connect(target: String?): String {
                    phone.connected.complete(Unit)
                    while (!gate.isCompleted) Thread.sleep(5)
                    return "connection-to-studio"
                }
            },
        )
        switch.request(null, "studio", null, phone.reply())
        runBlocking { withTimeout(5_000) { phone.connected.await() } }
        switch.cancel()
        gate.complete(Unit)
        phone.runMain(1)

        assertEquals(listOf("reply cancelled", "discard connection-to-studio"), phone.events)
        assertEquals(null, phone.selected)
    }

    @Test fun aSecondSwitchWaitsForTheFirst() {
        val phone = Phone()
        phone.switch.request(null, "studio", null, phone.reply())
        phone.switch.request(null, "laptop", null, phone.reply())
        assertEquals("reply Another computer is still connecting. Wait for it, or cancel it first.", phone.events.last { it.startsWith("reply") })
        phone.switch.cancel()
    }

    @Test fun theComputerInUseIsNotConnectedAgain() {
        val phone = Phone()
        phone.switch.request("studio", "studio", null, phone.reply())
        assertEquals(listOf("reply unchanged"), phone.events)
        assertFalse(phone.switch.switching)
    }

    @Test fun onlyAPlainlySafeOperationIdIsLogged() {
        assertEquals("computer-switch-1_a", ComputerSwitch.operationId("computer-switch-1_a"))
        listOf(null, "", "has space", "a/b", "x".repeat(81)).forEach {
            assertTrue(ComputerSwitch.operationId(it).startsWith("android-computer-switch-"))
        }
    }
}
