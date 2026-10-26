package io.openaide.android

import java.util.UUID
import kotlin.coroutines.cancellation.CancellationException
import kotlinx.coroutines.CoroutineScope
import kotlinx.coroutines.Job
import kotlinx.coroutines.isActive
import kotlinx.coroutines.launch

/**
 * The phone's one switch of the computer in use (ADR-0070). It connects the
 * target while the computer in use stays in use, and changes nothing until that
 * connection is made: a failure or a Cancel leaves the selection, the live
 * connection and the page as they were. Once connected it saves the choice,
 * answers the page, and only then replaces the page, so the answer is not lost
 * with it.
 *
 * Owned by one page's controller and called on the main thread; [main] brings
 * the connect's result back there. Kept free of Android and of the library so
 * the rules can be tested.
 */
internal class ComputerSwitch<C>(
    private val scope: CoroutineScope,
    private val main: (() -> Unit) -> Unit,
    private val clock: () -> Long,
    private val log: Log,
    private val steps: Steps<C>,
) {
    /** What a switch does to the phone; `null` as a target is this phone. */
    interface Steps<C> {
        /** Reaches the target without touching the computer in use; throws [Failed] with a sentence for the user. */
        suspend fun connect(target: String?): C
        /** Saves the choice and moves the phone's connection to what [connect] reached. */
        fun apply(target: String?, connected: C)
        /** Gives up what [connect] reached for a switch that no longer applies. */
        fun discard(connected: C)
        /** Replaces the page with the computer now in use. */
        fun replacePage()
    }

    fun interface Reply { fun done(outcome: String?, problem: String?) }

    fun interface Log { fun record(event: String, outcome: String, durationMs: Long, fields: Map<String, String>) }

    /** A target that could not be reached: a stable class for diagnostics, and a sentence for the user. */
    class Failed(val errorClass: String, message: String) : Exception(message)

    private class Pending(val operationId: String, val fields: Map<String, String>, val started: Long, val reply: Reply) {
        var job: Job? = null
    }

    private var pending: Pending? = null

    val switching: Boolean get() = pending != null

    /** Switches from [source] to [target]; [reply] answers once with `switched`, `unchanged`, `cancelled`, or a problem. */
    fun request(source: String?, target: String?, operationId: String?, reply: Reply) {
        if (pending != null) {
            reply.done(null, "Another computer is still connecting. Wait for it, or cancel it first.")
            return
        }
        if (source == target) {
            reply.done("unchanged", null)
            return
        }
        val fields = mapOf(
            "operation_id" to operationId(operationId),
            "source_kind" to kind(source),
            "target_kind" to kind(target),
        )
        val switch = Pending(fields.getValue("operation_id"), fields, clock(), reply)
        pending = switch
        log.record("computer_switch", "started", 0, fields)
        switch.job = scope.launch {
            val result = try {
                Result.success(steps.connect(target))
            } catch (error: CancellationException) {
                // A Cancel already answered the page; a timeout that escaped the connect did not.
                if (!isActive) return@launch
                Result.failure(error)
            } catch (error: Throwable) {
                Result.failure(error)
            }
            main { finish(switch, target, result) }
        }
    }

    /** Ends the switch still connecting; the page is answered `cancelled`. False when nothing was connecting. */
    fun cancel(): Boolean {
        val switch = pending ?: return false
        pending = null
        switch.job?.cancel()
        log.record("computer_switch", "cancelled", clock() - switch.started, switch.fields)
        switch.reply.done("cancelled", null)
        return true
    }

    private fun finish(switch: Pending, target: String?, result: Result<C>) {
        if (pending !== switch) {
            // Cancelled after the target answered: what it reached is not used.
            result.onSuccess { runCatching { steps.discard(it) } }
            return
        }
        pending = null
        val elapsed = clock() - switch.started
        val problem = result.fold(
            onSuccess = { connected ->
                try {
                    steps.apply(target, connected)
                    null
                } catch (error: Throwable) {
                    runCatching { steps.discard(connected) }
                    Failed("apply", "OpenAIDE could not save that choice. Try again.")
                }
            },
            onFailure = { it as? Failed ?: Failed("unexpected", "OpenAIDE could not switch. Try again.") },
        )
        if (problem != null) {
            log.record("computer_switch", "failed", elapsed, switch.fields + ("error_class" to problem.errorClass))
            switch.reply.done(null, problem.message)
            return
        }
        log.record("computer_switch", "switched", elapsed, switch.fields)
        switch.reply.done("switched", null)
        // The answer is posted before the page that would receive it is replaced.
        main { steps.replacePage() }
    }

    companion object {
        private val SAFE_ID = Regex("[A-Za-z0-9_-]{1,80}")

        /** The page's correlation id when it is plainly safe to log, else a new one. */
        fun operationId(candidate: String?): String =
            candidate?.takeIf { SAFE_ID.matches(it) } ?: "android-computer-switch-${UUID.randomUUID()}"

        /** An SSH Computer's target is its id behind this prefix; a paired computer's is its key. */
        const val SSH_PREFIX = "ssh:"

        /** The SSH Computer's id in [target], or null for this phone or a paired computer. */
        fun sshId(target: String?): String? = target?.takeIf { it.startsWith(SSH_PREFIX) }?.removePrefix(SSH_PREFIX)

        private fun kind(target: String?) = when {
            target == null -> "this_device"
            sshId(target) != null -> "ssh"
            else -> "paired"
        }
    }
}
