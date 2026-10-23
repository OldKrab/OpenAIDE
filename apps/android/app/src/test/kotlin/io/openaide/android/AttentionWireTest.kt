package io.openaide.android

import org.json.JSONObject
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class AttentionWireTest {
    private fun http(status: Int, body: String) = "HTTP/1.1 $status OK\r\nContent-Type: application/json\r\nContent-Length: ${body.length}\r\n\r\n$body".toByteArray()

    @Test fun requestIsOneClosedPostToTheBridge() {
        val request = String(AttentionWire.request("android-attention-1", "7", AttentionWire.WAIT, AttentionWire.waitParams("abc")), Charsets.UTF_8)
        val (head, body) = request.split("\r\n\r\n", limit = 2)

        assertTrue(head.startsWith("POST /probe HTTP/1.1\r\n"))
        assertTrue(head.contains("\r\nX-OpenAIDE-Connection-Id: android-attention-1"))
        assertTrue(head.contains("\r\nConnection: close"))
        assertTrue(head.contains("\r\nContent-Length: ${body.toByteArray().size}"))
        // The bridge adds the credential; the phone never sends one.
        assertTrue(!head.contains("Authorization", ignoreCase = true))
        val json = JSONObject(body)
        assertEquals("taskAttention/wait", json.getString("method"))
        assertEquals("abc", json.getJSONObject("params").getString("cursor"))
        assertEquals(0, AttentionWire.waitParams(null).length())
    }

    @Test fun readsTheHeldAnswer() {
        val body = """[{"jsonrpc":"2.0","method":"event"},{"jsonrpc":"2.0","id":"7","result":{"result":{"cursor":"c1","tasks":[
            {"taskId":"t1","title":{"value":"Fix build"},"attention":{"eventId":"e1","reason":"needsAnswer","occurredAt":"1767225600000"}},
            {"taskId":"t2","attention":null}]},"meta":{}}}]"""

        val answer = AttentionWire.answer(http(200, body), "7", AttentionWire.WAIT) as AttentionWire.Answer.Result
        val wait = AttentionWire.waitResult(answer.value)!!

        assertEquals("c1", wait.cursor)
        assertEquals(1, wait.attention.size)
        assertEquals("t1", wait.attention[0].taskId)
        assertEquals("Fix build", wait.attention[0].title)
        assertEquals("e1", wait.attention[0].eventId)
        assertEquals(1767225600000L, wait.attention[0].occurredAt)
        assertNull(AttentionWire.waitResult(JSONObject().put("tasks", org.json.JSONArray())))
    }

    @Test fun classifiesFailuresByWhatTheWatchMustDo() {
        fun error(code: String) = http(200, """{"jsonrpc":"2.0","id":"7","error":{"code":-32600,"message":"x","error":{"code":"$code"}}}""")

        assertTrue(AttentionWire.answer(error("notInitialized"), "7", AttentionWire.WAIT) is AttentionWire.Answer.NotInitialized)
        // Only the new method marks an App Server as older than this app.
        assertTrue(AttentionWire.answer(error("invalidRequest"), "7", AttentionWire.WAIT) is AttentionWire.Answer.Unsupported)
        assertEquals("error_invalidRequest", (AttentionWire.answer(error("invalidRequest"), "7", "client/initialize") as AttentionWire.Answer.Failed).cause)
        assertEquals("error", (AttentionWire.answer(error("not a code"), "7", AttentionWire.WAIT) as AttentionWire.Answer.Failed).cause)
        assertEquals("http_502", (AttentionWire.answer(http(502, ""), "7", AttentionWire.WAIT) as AttentionWire.Answer.Failed).cause)
        assertEquals("invalid_response", (AttentionWire.answer("garbage".toByteArray(), "7", AttentionWire.WAIT) as AttentionWire.Answer.Failed).cause)
        assertEquals("missing_response", (AttentionWire.answer(http(200, "[]"), "7", AttentionWire.WAIT) as AttentionWire.Answer.Failed).cause)
    }

    @Test fun retriesAtOnceThenBacksOff() {
        assertEquals(0L, AttentionWire.retryDelayMs(1))
        assertEquals(2_000L, AttentionWire.retryDelayMs(2))
        assertEquals(60_000L, AttentionWire.retryDelayMs(6))
        assertEquals(60_000L, AttentionWire.retryDelayMs(500))
    }
}
