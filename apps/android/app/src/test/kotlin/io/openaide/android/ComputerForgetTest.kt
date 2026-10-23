package io.openaide.android

import kotlinx.coroutines.CompletableDeferred
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Dispatchers
import kotlinx.coroutines.SupervisorJob
import kotlinx.coroutines.runBlocking
import kotlinx.coroutines.withTimeout
import org.junit.Assert.assertEquals
import org.junit.Assert.assertTrue
import org.junit.Test

class ComputerForgetTest {
    /** A phone paired with two computers and using one, with a real switch over fake steps. */
    private class Phone(var inUse: String? = "studio") {
        val paired = mutableListOf("studio", "laptop")
        val events: MutableList<String> = java.util.Collections.synchronizedList(mutableListOf())
        val logged = mutableListOf<Pair<String, Map<String, String>>>()
        val switchLogged = mutableListOf<Pair<String, Map<String, String>>>()
        val answer = CompletableDeferred<Result<String>>()
        val connected = CompletableDeferred<Unit>()
        val mainQueue = java.util.concurrent.LinkedBlockingQueue<() -> Unit>()

        val switch = ComputerSwitch(
            CoroutineScope(SupervisorJob() + Dispatchers.Default),
            { mainQueue.put(it) },
            { 0L },
            { _, outcome, _, fields -> synchronized(switchLogged) { switchLogged += outcome to fields } },
            object : ComputerSwitch.Steps<String> {
                override suspend fun connect(target: String?): String {
                    events += "connect $target"
                    connected.complete(Unit)
                    return answer.await().getOrThrow()
                }
                override fun apply(target: String?, connected: String) {
                    events += "apply $target"
                    inUse = target
                }
                override fun discard(connected: String) { events += "discard" }
                override fun replacePage() { events += "replace page" }
            },
        )

        val pairings = object : ComputerForget.Pairings {
            override fun inUse() = inUse
            override fun forget(id: String) {
                events += "forget $id"
                paired -= id
            }
        }

        fun forget(id: String, operationId: String? = "forget-1") = ComputerForget.forget(
            id, operationId, pairings, switch, { outcome, fields -> logged += outcome to fields },
            { events += "redraw" }, { outcome, problem -> events += "reply ${outcome ?: problem}" },
        )

        fun awaitConnect() = runBlocking { withTimeout(5_000) { connected.await() } }

        fun runMain(count: Int) = repeat(count) { mainQueue.poll(5, java.util.concurrent.TimeUnit.SECONDS)!!.invoke() }
    }

    @Test fun aComputerNotInUseIsForgottenAtOnce() {
        val phone = Phone()
        phone.forget("laptop")
        assertEquals(listOf("forget laptop", "redraw", "reply forgotten"), phone.events)
        assertEquals(listOf("studio"), phone.paired)
        assertEquals("studio", phone.inUse)
        assertEquals(listOf("other"), phone.logged.map { it.first })
        assertTrue(phone.switchLogged.isEmpty())
    }

    @Test fun theComputerInUseIsForgottenOnlyOnceThePhoneIsInUse() {
        val phone = Phone()
        phone.forget("studio")
        phone.awaitConnect()
        assertEquals(listOf("connect null"), phone.events)
        assertEquals(listOf("studio", "laptop"), phone.paired)

        phone.answer.complete(Result.success("phone"))
        phone.runMain(2)
        assertEquals(listOf("connect null", "apply null", "forget studio", "reply switched", "replace page"), phone.events)
        assertEquals(null, phone.inUse)
        assertEquals(listOf("laptop"), phone.paired)
        // The forget and the switch it rode on share one correlation id.
        assertEquals(listOf("current" to mapOf("operation_id" to "forget-1")), phone.logged)
        assertEquals(listOf("started", "switched"), phone.switchLogged.map { it.first })
        assertTrue(phone.switchLogged.all { it.second["operation_id"] == "forget-1" })
    }

    @Test fun aCancelKeepsThePairing() {
        val phone = Phone()
        phone.forget("studio")
        phone.awaitConnect()
        phone.switch.cancel()
        assertEquals(listOf("connect null", "reply cancelled"), phone.events)
        assertEquals(listOf("studio", "laptop"), phone.paired)
        assertEquals("studio", phone.inUse)
        assertEquals(listOf("kept"), phone.logged.map { it.first })
    }

    @Test fun aPhoneThatCannotStartChangesNothingAndSaysWhy() {
        val phone = Phone()
        phone.forget("studio", operationId = "unsafe id!")
        phone.answer.complete(Result.failure(ComputerSwitch.Failed("local_unavailable", "Your local workspace could not start.")))
        phone.runMain(1)
        assertEquals(listOf("connect null", "reply Your local workspace could not start."), phone.events)
        assertEquals(listOf("studio", "laptop"), phone.paired)
        assertEquals("studio", phone.inUse)
        assertTrue(phone.logged.isEmpty())
        // An id unsafe to log is replaced, and the switch still logs under one id.
        val ids = phone.switchLogged.map { it.second.getValue("operation_id") }.toSet()
        assertEquals(1, ids.size)
        assertTrue(ids.single().startsWith("android-computer-switch-"))
    }
}
