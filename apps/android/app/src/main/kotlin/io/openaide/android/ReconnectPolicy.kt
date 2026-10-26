package io.openaide.android

/**
 * When the paired connection starts over, and how long it may take. Kept free of
 * Android and of the library so the rules can be tested.
 *
 * Android stops an app's network shortly after it leaves the screen, and the
 * library cannot observe that: its endpoint keeps retrying a dead relay link at
 * growing intervals, so the next dial waits out whatever interval is running. A
 * new endpoint dials at once, which makes replacing it cheaper than waiting.
 */
internal object ReconnectPolicy {
    /** Shorter than the delay after which Android was seen to stop a hidden app's network. */
    const val STALE_AFTER_HIDDEN_MS = 3_000L

    /** All attempts of one connect share this; the user sees "Connecting…" throughout. */
    const val CONNECT_BUDGET_MS = 24_000L

    /** A new endpoint connects in well under a second; later attempts allow for a slow network. */
    private val ATTEMPT_TIMEOUTS_MS = longArrayOf(4_000L, 8_000L, 12_000L)

    /** Too little left to finish a handshake is not worth another endpoint. */
    private const val SHORTEST_ATTEMPT_MS = 1_000L

    private const val RELAY_LIMIT = 200

    fun staleAfterHidden(hiddenMs: Long): Boolean = hiddenMs > STALE_AFTER_HIDDEN_MS

    /**
     * Whether a failed dial gives the next attempt a new endpoint. Not while the
     * endpoint carries the connection in use, as when a switch to another computer
     * dials first: retiring it would end the computer the user is still on.
     */
    fun retireAfterFailedDial(carriesLiveConnection: Boolean): Boolean = !carriesLiveConnection

    /**
     * Whether an endpoint marked stale is given up now. Not while a switch is dialling on
     * it: that would end the switch's attempt, spending its budget on a reachable computer
     * or failing it outright when the app is hidden. The retire waits for the dial to end.
     */
    fun retireNow(switchDialsInFlight: Int): Boolean = switchDialsInFlight == 0

    /** How long attempt [index] (from zero) may take after [spentMs] of the budget, or zero for no further attempt. */
    fun attemptTimeoutMs(index: Int, spentMs: Long): Long {
        if (index < 0 || index >= ATTEMPT_TIMEOUTS_MS.size) return 0
        val remaining = CONNECT_BUDGET_MS - spentMs
        if (remaining < SHORTEST_ATTEMPT_MS) return 0
        return minOf(ATTEMPT_TIMEOUTS_MS[index], remaining)
    }

    /**
     * The relay the computer was last reached through, which spares the first
     * attempt an address lookup. Later attempts dial by key alone, so an address
     * that went out of date cannot keep the computer unreachable.
     */
    fun relayFor(index: Int, remembered: String?): String? = if (index == 0 && validRelay(remembered)) remembered else null

    fun validRelay(url: String?): Boolean =
        url != null && url.length <= RELAY_LIMIT && url.startsWith("https://") && url.none { it.isWhitespace() || it.isISOControl() }
}
