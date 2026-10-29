package io.openaide.android

/**
 * One attempt to reach a paired computer, written as the single terminal `remote_connect`
 * record of that attempt. A shared report then says, per line, which computer was dialled,
 * for what, how its address was found, what this phone's own network looked like, how far
 * the attempt got and what ended it, without matching lines up by attempt number.
 *
 * Metadata only: fixed words, counts, and the first characters of the computer's public
 * endpoint id. An exception's message is never written; [errorReason] maps it to a word.
 */
internal class DialReport(
    private val purpose: Purpose,
    private val index: Int,
    serverId: String,
    private val network: NetworkKind,
) {
    /** Why the phone dialled: the connection in use, a switch to another computer, or a check. */
    enum class Purpose { LIVE, SWITCH, CHECK }

    /** How far the attempt got. The stage of a failed attempt is the one that failed. */
    enum class Stage { BIND, ADDRESS, CONNECT, REFUSAL_CHECK, DONE }

    private val target = shortId(serverId)
    var stage = Stage.BIND
    /** `remembered_relay` when the stored relay was tried first, `lookup` when the address was looked up. */
    var address = "none"
    /** Whether this phone's own endpoint had a home relay when it dialled. */
    var ownRelay: Boolean? = null
    /** What the endpoint knew of the computer's address when the attempt failed. */
    var knownRelay: Boolean? = null
    var knownDirect: Int? = null

    fun startFields(): Map<String, Any> = mapOf("purpose" to purpose.name.lowercase(), "target" to target, "index" to index)

    fun fields(error: Throwable?): Map<String, Any> = buildMap {
        putAll(startFields())
        putAll(network.fields())
        put("stage", stage.name.lowercase())
        put("address", address)
        ownRelay?.let { put("own_relay", if (it) "present" else "absent") }
        knownRelay?.let { put("known_relay", if (it) "present" else "absent") }
        knownDirect?.let { put("known_direct", it) }
        if (error != null) {
            put("error_class", errorClass(error))
            errorReason(error.message)?.let { put("error_reason", it) }
        }
    }

    companion object {
        /** Enough of a public endpoint id to tell paired computers apart in one report. */
        fun shortId(id: String): String = id.filter { it.isLetterOrDigit() }.take(8).ifEmpty { "none" }

        /** The exception's type, so an unexpected failure is named rather than folded into "unreachable". */
        fun errorClass(error: Throwable): String = error::class.simpleName ?: "anonymous"

        /**
         * A fixed word for the library's error text, whose wording is not a contract: the
         * text itself may carry addresses, so only these words reach the report.
         */
        fun errorReason(message: String?): String? {
            val text = message?.lowercase() ?: return null
            return when {
                // TLS alert 120 (no_application_protocol): the computer offers no ALPN this app
                // speaks, so it runs another protocol major. The text never says "ALPN".
                "alpn" in text || "error 120" in text || "known protocol" in text -> "alpn"
                "address" in text || "discovery" in text -> "no_address"
                "relay" in text -> "relay"
                "timed out" in text || "timeout" in text -> "timed_out"
                "closed" in text || "lost" in text || "reset" in text -> "closed"
                else -> "other"
            }
        }
    }
}

/**
 * The phone's own network when it dialled: the transport Android routes through, whether
 * Android validated it as reaching the internet, and whether a VPN carries it.
 */
internal data class NetworkKind(val transport: String, val validated: Boolean, val vpn: Boolean) {
    fun fields(): Map<String, Any> = mapOf("network" to transport, "network_validated" to validated, "vpn" to vpn)

    companion object {
        val NONE = NetworkKind("none", validated = false, vpn = false)
        val UNKNOWN = NetworkKind("unknown", validated = false, vpn = false)

        /** Names the transport; a VPN is reported beside the transport under it, when Android says which. */
        fun of(wifi: Boolean, cellular: Boolean, ethernet: Boolean, vpn: Boolean, validated: Boolean): NetworkKind {
            val transport = when {
                wifi -> "wifi"
                cellular -> "cellular"
                ethernet -> "ethernet"
                vpn -> "vpn"
                else -> "other"
            }
            return NetworkKind(transport, validated, vpn)
        }
    }
}
