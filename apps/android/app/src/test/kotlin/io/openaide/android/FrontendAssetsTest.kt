package io.openaide.android

import org.junit.Assert.assertEquals
import org.junit.Assert.assertNull
import org.junit.Assert.assertTrue
import org.junit.Test

class FrontendAssetsTest {
    @Test fun productRoutesOpenTheSameDocumentOnTheirSurface() {
        assertEquals("task", FrontendAssets.route("/")!!.surface)
        assertEquals("task", FrontendAssets.route("/new-task")!!.surface)
        assertTrue(FrontendAssets.route("/archive")!!.archived)
        assertEquals("settings", FrontendAssets.route("/settings/")!!.surface)
        assertEquals("task-1", FrontendAssets.route("/task/task-1")!!.taskId)
        assertNull(FrontendAssets.route("/task")!!.taskId)
        val session = FrontendAssets.route("/session/codex/a%20b")!!
        assertEquals("nativeSession", session.surface)
        assertEquals("codex", session.agentId)
        assertEquals("a b", session.nativeSessionId)
        assertNull(FrontendAssets.route("/session/codex/%zz"))
        assertNull(FrontendAssets.route("/assets/index.js"))
    }

    @Test fun assetsNeverLeaveTheBundle() {
        assertEquals("frontend/assets/index.js", FrontendAssets.asset("/assets/index.js"))
        for (path in listOf("/../setup/app.js", "/assets/../../x", "/assets//x", "/assets/%2e%2e/x", "/assets\\x", "/")) {
            assertNull(FrontendAssets.asset(path))
        }
    }

    @Test fun bootstrapNamesTheSurfaceAndTheGatewayRoute() {
        val html = FrontendAssets.withBootstrap("<html><body class=\"app\"><div id=\"root\"></div></body></html>",
            FrontendAssets.Route("task", taskId = "a\"<b"))
        assertTrue(html.contains("<body class=\"app\" data-shell=\"web\" data-navigation-mode=\"project\" data-surface=\"task\""))
        assertTrue(html.contains("data-task-id=\"a&quot;&lt;b\""))
        assertTrue(html.contains("data-app-server-connection=\"{&quot;kind&quot;:&quot;webProxy&quot;,&quot;endpointUrl&quot;:&quot;/__openaide-app-server/probe&quot;,&quot;transport&quot;:&quot;webSocket&quot;}\""))
    }

    @Test fun scriptsAndStylesAreServedWithTypesTheWebViewExecutes() {
        assertEquals("text/javascript; charset=utf-8", FrontendAssets.contentType("frontend/assets/index.js"))
        assertEquals("text/css; charset=utf-8", FrontendAssets.contentType("frontend/assets/index.css"))
        assertEquals("font/woff2", FrontendAssets.contentType("frontend/assets/a.woff2"))
        assertEquals("application/octet-stream", FrontendAssets.contentType("frontend/unknown"))
    }
}
