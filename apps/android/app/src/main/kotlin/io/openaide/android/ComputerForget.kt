package io.openaide.android

/**
 * Forgetting a paired computer (ADR-0070). Forgetting the computer in use moves the
 * phone to itself through the connect-first [ComputerSwitch] and forgets the pairing
 * only once that switch took effect: a Cancel keeps the pairing, and a failure answers
 * why and changes nothing. Forgetting a computer not in use is immediate.
 *
 * Called on the main thread; kept free of Android so the rules can be tested.
 */
internal object ComputerForget {
    /** The phone's pairings as forgetting needs them. */
    interface Pairings {
        /** The paired computer in use, or null while this phone is. */
        fun inUse(): String?
        fun forget(id: String)
    }

    fun interface Log { fun record(outcome: String, fields: Map<String, String>) }

    /** Forgets [id]; [redraw] shows the shorter list when the page stays, and [reply] answers once. */
    fun <C> forget(id: String, operationId: String?, pairings: Pairings, switch: ComputerSwitch<C>, log: Log, redraw: Runnable, reply: ComputerSwitch.Reply) {
        val inUse = pairings.inUse()
        if (inUse != id) {
            pairings.forget(id)
            log.record("other", emptyMap())
            redraw.run()
            reply.done("forgotten", null)
            return
        }
        // The switch logs under the same id, so its events and the forget correlate.
        val operation = ComputerSwitch.operationId(operationId)
        val fields = mapOf("operation_id" to operation)
        switch.request(inUse, null, operation) { outcome, problem ->
            if (outcome != "switched") {
                // Cancelled, or not switched: the computer stays paired and in use.
                if (problem == null) log.record("kept", fields + ("cause" to (outcome ?: "unknown")))
                reply.done(outcome, problem)
                return@request
            }
            val forgotten = runCatching { pairings.forget(id) }.isSuccess
            if (forgotten) log.record("current", fields) else log.record("failed", fields + ("error_class" to "store"))
            // The switch replaces the page after this answer.
            if (forgotten) reply.done("switched", null)
            else reply.done(null, "This phone is now in use, but OpenAIDE could not forget that computer. Try again.")
        }
    }
}
