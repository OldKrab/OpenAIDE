package io.openaide.android

/**
 * The HTTP rules of the workspace gateway, kept free of sockets.
 *
 * The WebView reaches the gateway on loopback, where every app on the phone can
 * connect too. A request is served only when it carries the per-process cookie
 * the shell gave its own WebView and names the gateway's exact origin.
 */
internal object GatewayHttp {
    const val COOKIE = "openaide_gateway"
    private const val PROXY_PREFIX = "/__openaide-app-server"

    class Head(val method: String, val target: String, val headers: List<Pair<String, String>>) {
        val path: String get() = target.substringBefore('?')
        fun header(name: String): String? = headers.firstOrNull { it.first.equals(name, ignoreCase = true) }?.second
        val upgrade: Boolean get() = header("upgrade") != null
    }

    fun parse(head: String): Head? {
        val lines = head.split("\r\n").filter { it.isNotEmpty() }
        val request = lines.firstOrNull()?.split(' ') ?: return null
        if (request.size != 3 || !request[2].startsWith("HTTP/1.") || !request[1].startsWith("/")) return null
        val headers = lines.drop(1).map { line ->
            val separator = line.indexOf(':')
            // A name with whitespace is how a second credential is smuggled past a filter.
            if (separator <= 0 || line.substring(0, separator).any { it.isWhitespace() }) return null
            line.substring(0, separator) to line.substring(separator + 1).trim()
        }
        return Head(request[0], request[1], headers)
    }

    fun authorized(head: Head, token: String, port: Int): Boolean {
        val origin = "127.0.0.1:$port"
        if (head.header("host") != origin) return false
        // A page of another origin may reach loopback, but never as this origin.
        head.header("origin")?.let { if (it != "http://$origin") return false }
        return (head.header("cookie") ?: "").split(';').any { it.trim() == "$COOKIE=$token" }
    }

    /**
     * Maps the Web Shell's App Server routes onto the App Server's own paths, or
     * returns null for a request the bundled Frontend answers.
     */
    fun appServerTarget(head: Head): String? {
        val path = head.path
        if (path != PROXY_PREFIX && !path.startsWith("$PROXY_PREFIX/")) return null
        val query = head.target.substring(path.length)
        val suffix = when {
            head.method == "POST" && path.endsWith("/upload/chunk") -> "/upload/chunk"
            head.method == "POST" && path.endsWith("/upload") -> "/upload"
            head.method == "GET" && path.endsWith("/download") -> "/download"
            else -> ""
        }
        return "/probe$suffix$query"
    }

    /** The same target on the local App Server, whose endpoint names its own base path. */
    fun localTarget(target: String, basePath: String): String = basePath + target.removePrefix("/probe")

    /**
     * The request as the App Server receives it. Nothing the WebView holds for the
     * gateway travels past it: a paired computer trusts the device by its key, and
     * the local App Server gets the token only the shell knows, as the first header
     * because the listener reads the first of a name.
     */
    fun forwarded(head: Head, target: String, localToken: String? = null): ByteArray {
        val text = StringBuilder("${head.method} $target HTTP/1.1\r\n")
        if (localToken != null) text.append("Authorization: Bearer ").append(localToken).append("\r\n")
        text.append("Host: 127.0.0.1\r\n")
        for ((name, value) in head.headers) {
            when (name.lowercase()) {
                "host", "cookie", "authorization", "origin", "referer" -> continue
                "connection" -> if (!head.upgrade) continue
            }
            text.append(name).append(": ").append(value).append("\r\n")
        }
        // One stream carries one request, so the response ends with the stream.
        if (!head.upgrade) text.append("Connection: close\r\n")
        return text.append("\r\n").toString().toByteArray(Charsets.ISO_8859_1)
    }

    fun response(status: Int, reason: String, type: String, body: ByteArray, headOnly: Boolean = false): ByteArray {
        val head = "HTTP/1.1 $status $reason\r\nContent-Type: $type\r\nContent-Length: ${body.size}\r\n" +
            "Cache-Control: no-store\r\nX-Content-Type-Options: nosniff\r\nConnection: close\r\n\r\n"
        return head.toByteArray(Charsets.ISO_8859_1) + if (headOnly) ByteArray(0) else body
    }
}
