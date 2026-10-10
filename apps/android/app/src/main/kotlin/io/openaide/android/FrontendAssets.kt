package io.openaide.android

/**
 * Serves the Frontend shipped in the APK the way the Web Shell serves it: every
 * product route is the same document, told which surface to open by attributes
 * on its body (`apps/web/src/dev-server-routes.mjs`).
 */
internal object FrontendAssets {
    const val ROOT = "frontend"

    class Route(
        val surface: String,
        val taskId: String? = null,
        val agentId: String? = null,
        val nativeSessionId: String? = null,
        val archived: Boolean = false,
    )

    private val nativeSession = Regex("^/session/([^/]+)/([^/]+)/?$")
    private val routes = listOf(
        Regex("^/(?:new-task)?$") to Route("task"),
        Regex("^/archive/?$") to Route("task", archived = true),
        Regex("^/settings/?$") to Route("settings"),
        Regex("^/task/([^/]+)/?$") to Route("task"),
        Regex("^/task/?$") to Route("task"),
    )

    fun route(path: String): Route? {
        nativeSession.find(path)?.let { match ->
            val agentId = decoded(match.groupValues[1]) ?: return null
            val nativeSessionId = decoded(match.groupValues[2]) ?: return null
            return Route("nativeSession", agentId = agentId, nativeSessionId = nativeSessionId)
        }
        for ((pattern, route) in routes) {
            val match = pattern.find(path) ?: continue
            return Route(route.surface, taskId = match.groupValues.getOrNull(1)?.takeIf { it.isNotEmpty() }, archived = route.archived)
        }
        return null
    }

    /** The asset behind a non-route path, or null when the path could leave the bundle. */
    fun asset(path: String): String? {
        val segments = path.trimStart('/').split('/')
        if (segments.any { it.isEmpty() || it == "." || it == ".." || it.contains('\\') || it.contains('%') }) return null
        return "$ROOT/${segments.joinToString("/")}"
    }

    fun withBootstrap(html: String, route: Route): String {
        val connection = """{"kind":"webProxy","endpointUrl":"/__openaide-app-server/probe","transport":"webSocket"}"""
        val attributes = listOfNotNull(
            """data-shell="web"""",
            """data-navigation-mode="project"""",
            """data-surface="${route.surface}"""",
            route.taskId?.let { """data-task-id="${escaped(it)}"""" },
            route.agentId?.let { """data-agent-id="${escaped(it)}"""" },
            route.nativeSessionId?.let { """data-native-session-id="${escaped(it)}"""" },
            if (route.archived) """data-archived="true"""" else null,
            """data-app-server-connection="${escaped(connection)}"""",
        ).joinToString(" ")
        val body = Regex("<body([^>]*)>", RegexOption.IGNORE_CASE).find(html) ?: return html
        return html.replaceRange(body.range, "<body${body.groupValues[1]} $attributes>")
    }

    fun contentType(asset: String): String = when (asset.substringAfterLast('.', "").lowercase()) {
        "html" -> "text/html; charset=utf-8"
        "js", "mjs" -> "text/javascript; charset=utf-8"
        "css" -> "text/css; charset=utf-8"
        "json", "map" -> "application/json; charset=utf-8"
        "svg" -> "image/svg+xml"
        "png" -> "image/png"
        "jpg", "jpeg" -> "image/jpeg"
        "gif" -> "image/gif"
        "webp" -> "image/webp"
        "ico" -> "image/x-icon"
        "woff" -> "font/woff"
        "woff2" -> "font/woff2"
        "ttf" -> "font/ttf"
        "wasm" -> "application/wasm"
        "txt" -> "text/plain; charset=utf-8"
        else -> "application/octet-stream"
    }

    private fun decoded(segment: String): String? =
        try { java.net.URLDecoder.decode(segment.replace("+", "%2B"), "UTF-8") } catch (error: IllegalArgumentException) { null }

    private fun escaped(value: String): String =
        value.replace("&", "&amp;").replace("\"", "&quot;").replace("<", "&lt;").replace(">", "&gt;")
}
