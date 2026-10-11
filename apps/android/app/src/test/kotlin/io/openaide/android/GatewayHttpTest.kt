package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertFalse
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class GatewayHttpTest {
    private fun head(request: String, vararg headers: String) =
        GatewayHttp.parse((listOf(request) + headers).joinToString("\r\n"))!!

    private val trusted = arrayOf("Host: 127.0.0.1:5475", "Cookie: theme=dark; openaide_gateway=token")

    @Test fun servesOnlyTheShellsOwnWebView() {
        assertTrue(GatewayHttp.authorized(head("GET / HTTP/1.1", *trusted), "token", 5475))
        assertTrue(GatewayHttp.authorized(head("POST /x HTTP/1.1", *trusted, "Origin: http://127.0.0.1:5475"), "token", 5475))
        // Another app on the phone can connect, but holds no cookie.
        assertFalse(GatewayHttp.authorized(head("GET / HTTP/1.1", "Host: 127.0.0.1:5475"), "token", 5475))
        assertFalse(GatewayHttp.authorized(head("GET / HTTP/1.1", "Host: 127.0.0.1:5475", "Cookie: openaide_gateway=tokens"), "token", 5475))
        // A rebinding name or a foreign page must not act as this origin.
        assertFalse(GatewayHttp.authorized(head("GET / HTTP/1.1", "Host: evil.example:5475", trusted[1]), "token", 5475))
        assertFalse(GatewayHttp.authorized(head("POST /x HTTP/1.1", *trusted, "Origin: https://evil.example"), "token", 5475))
    }

    @Test fun mapsWebShellRoutesOntoAppServerPaths() {
        fun target(request: String) = GatewayHttp.appServerTarget(head(request, *trusted))
        assertEquals("/probe", target("POST /__openaide-app-server/probe HTTP/1.1"))
        assertEquals("/probe?connectionId=a%20b", target("GET /__openaide-app-server/probe?connectionId=a%20b HTTP/1.1"))
        assertEquals("/probe/upload?taskId=1", target("POST /__openaide-app-server/upload?taskId=1 HTTP/1.1"))
        assertEquals("/probe/upload/chunk", target("POST /__openaide-app-server/upload/chunk HTTP/1.1"))
        assertEquals("/probe/download?messageId=m", target("GET /__openaide-app-server/download?messageId=m HTTP/1.1"))
        assertNull(target("GET /assets/index.js HTTP/1.1"))
        assertNull(target("GET /__openaide-app-server-other HTTP/1.1"))
    }

    @Test fun forwardsNoGatewayCredentialAndEndsTheResponseWithTheStream() {
        val request = head("POST /__openaide-app-server/probe HTTP/1.1", *trusted, "Authorization: Bearer stolen",
            "Origin: http://127.0.0.1:5475", "Connection: keep-alive", "Content-Length: 2", "x-openaide-connection-id: c1")
        val forwarded = String(GatewayHttp.forwarded(request, "/probe"), Charsets.ISO_8859_1)
        assertTrue(forwarded.startsWith("POST /probe HTTP/1.1\r\nHost: 127.0.0.1\r\n"))
        assertTrue(forwarded.endsWith("Connection: close\r\n\r\n"))
        assertTrue(forwarded.contains("Content-Length: 2\r\n"))
        assertTrue(forwarded.contains("x-openaide-connection-id: c1\r\n"))
        for (secret in listOf("token", "stolen", "5475", "keep-alive")) assertFalse(forwarded.contains(secret))
    }

    @Test fun givesTheLocalAppServerOnlyTheShellsToken() {
        val request = head("POST /__openaide-app-server/upload?taskId=1 HTTP/1.1", *trusted, "Authorization: Bearer stolen", "Content-Length: 2")
        val target = GatewayHttp.localTarget(GatewayHttp.appServerTarget(request)!!, "/rpc")
        assertEquals("/rpc/upload?taskId=1", target)
        val forwarded = String(GatewayHttp.forwarded(request, target, "local-token"), Charsets.ISO_8859_1)
        // The listener reads the first header of a name, so the shell's credential leads.
        assertTrue(forwarded.startsWith("POST /rpc/upload?taskId=1 HTTP/1.1\r\nAuthorization: Bearer local-token\r\nHost: 127.0.0.1\r\n"))
        assertFalse(forwarded.contains("stolen"))
        assertFalse(forwarded.contains("openaide_gateway"))
    }

    @Test fun keepsAWebSocketHandshakeIntact() {
        val request = head("GET /__openaide-app-server/probe?connectionId=c1 HTTP/1.1", *trusted, "Connection: Upgrade",
            "Upgrade: websocket", "Sec-WebSocket-Key: key", "Sec-WebSocket-Version: 13")
        val forwarded = String(GatewayHttp.forwarded(request, "/probe?connectionId=c1"), Charsets.ISO_8859_1)
        for (line in listOf("Connection: Upgrade", "Upgrade: websocket", "Sec-WebSocket-Key: key")) assertTrue(forwarded.contains("$line\r\n"))
        assertFalse(forwarded.contains("Connection: close"))
    }

    @Test fun rejectsRequestHeadsThatCouldHideAHeader() {
        assertNull(GatewayHttp.parse("GET / HTTP/1.1\r\nHost : 127.0.0.1:5475"))
        assertNull(GatewayHttp.parse("GET / HTTP/1.1\r\nno-separator"))
        assertNull(GatewayHttp.parse("GET http://evil.example/ HTTP/1.1"))
        assertNull(GatewayHttp.parse("GET /"))
        assertNotNull(GatewayHttp.parse("GET / HTTP/1.1"))
    }
}
