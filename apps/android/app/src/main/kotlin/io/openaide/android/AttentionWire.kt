package io.openaide.android

import org.json.JSONArray
import org.json.JSONException
import org.json.JSONObject

/**
 * The exchanges the background service has with an App Server about Task
 * Attention Events, kept free of Android and of the library so they can be tested.
 *
 * A paired computer is asked with `taskAttention/wait` (ADR-0027): App Server
 * holds a request whose cursor still names the current set until the set changes
 * or about 25 seconds pass, so a watch that has nothing to report costs one small
 * exchange per wait.
 */
internal object AttentionWire {
    const val WAIT = "taskAttention/wait"
    const val MAX_RESPONSE_BYTES = 1024 * 1024

    /** The delays before each retry of a watch that keeps failing; the first retry is at once. */
    private val RETRY_DELAYS_MS = longArrayOf(0, 2_000, 5_000, 15_000, 30_000, 60_000)

    /** How one exchange ended. */
    sealed interface Answer {
        class Result(val value: JSONObject) : Answer
        /** App Server forgot this client, as after its restart; it must initialize again. */
        object NotInitialized : Answer
        /** App Server does not know the method: it is older than this app. */
        object Unsupported : Answer
        /** Any other failure, by a fixed word that is safe to record. */
        class Failed(val cause: String) : Answer
    }

    class Wait(val cursor: String, val attention: List<AttentionNotifications.Attention>)

    /** The delay before retry [failures] (from one) of a watch that keeps failing. */
    fun retryDelayMs(failures: Int): Long = RETRY_DELAYS_MS[(failures - 1).coerceIn(0, RETRY_DELAYS_MS.size - 1)]

    /** One JSON-RPC request as the paired computer's device bridge expects it: one request per stream. */
    fun request(connectionId: String, id: String, method: String, params: JSONObject): ByteArray {
        val body = JSONObject().put("jsonrpc", "2.0").put("id", id).put("method", method).put("params", params)
            .toString().toByteArray(Charsets.UTF_8)
        val head = "POST /probe HTTP/1.1\r\nHost: 127.0.0.1\r\nContent-Type: application/json\r\n" +
            "Content-Length: ${body.size}\r\nX-OpenAIDE-Connection-Id: $connectionId\r\nConnection: close\r\n\r\n"
        return head.toByteArray(Charsets.ISO_8859_1) + body
    }

    /** The client the background service is to App Server: a phone shell that renders nothing. */
    fun initializeParams(clientId: String): JSONObject = JSONObject()
        .put("clientInstanceId", clientId)
        .put("shell", JSONObject().put("kind", "mobile"))
        .put("requestedSurface", JSONObject().put("kind", "home"))
        .put("capabilities", JSONObject()
            .put("protocol", JSONArray(listOf("requestResponses", "stableClientRequestIds", "resync")))
            .put("shell", JSONArray()))

    fun waitParams(cursor: String?): JSONObject = JSONObject().apply { if (cursor != null) put("cursor", cursor) }

    /** Reads the HTTP response to request [id]. */
    fun answer(response: ByteArray, id: String, method: String): Answer {
        val text = String(response, Charsets.UTF_8)
        val headEnd = text.indexOf("\r\n\r\n")
        if (headEnd < 0) return Answer.Failed("invalid_response")
        val status = text.substring(0, headEnd).lineSequence().first().split(' ').getOrNull(1)?.toIntOrNull()
            ?: return Answer.Failed("invalid_response")
        if (status != 200) return Answer.Failed("http_$status")
        val body = text.substring(headEnd + 4).trim()
        return try {
            val message = if (body.startsWith("[")) {
                val messages = JSONArray(body)
                (0 until messages.length()).map(messages::getJSONObject).firstOrNull { it.optString("id") == id }
            } else JSONObject(body)
            if (message == null) return Answer.Failed("missing_response")
            val error = message.optJSONObject("error")
            if (error != null) {
                val code = error.optJSONObject("error")?.optString("code") ?: ""
                return when {
                    code == "notInitialized" -> Answer.NotInitialized
                    // An App Server older than the method rejects it as an invalid request.
                    code == "invalidRequest" && method == WAIT -> Answer.Unsupported
                    else -> Answer.Failed(if (code.matches(Regex("[A-Za-z]{1,40}"))) "error_$code" else "error")
                }
            }
            // A protocol result arrives wrapped in the envelope's own result.
            val result = message.optJSONObject("result") ?: return Answer.Result(JSONObject())
            Answer.Result(result.optJSONObject("result") ?: result)
        } catch (error: JSONException) {
            Answer.Failed("invalid_response")
        }
    }

    /** The set a `taskAttention/wait` answered with; null when the answer is malformed. */
    fun waitResult(result: JSONObject): Wait? {
        val cursor = result.optString("cursor").ifEmpty { return null }
        val tasks = result.optJSONArray("tasks") ?: return null
        val attention = (0 until tasks.length()).mapNotNull { index -> tasks.optJSONObject(index)?.let(::attention) }
        return Wait(cursor, attention)
    }

    /** The current Task Attention Event of a task summary, or null when it has none. */
    fun attention(task: JSONObject): AttentionNotifications.Attention? {
        val event = task.optJSONObject("attention") ?: return null
        val taskId = task.optString("taskId").ifEmpty { return null }
        val eventId = event.optString("eventId").ifEmpty { return null }
        val title = task.optJSONObject("title")?.optString("value")?.ifEmpty { null } ?: "Untitled task"
        return AttentionNotifications.Attention(taskId, title, eventId, event.optString("reason"),
            AttentionNotifications.timestamp(event.optString("occurredAt")))
    }
}
