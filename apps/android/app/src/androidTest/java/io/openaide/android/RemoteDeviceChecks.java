package io.openaide.android;

import android.test.InstrumentationTestCase;
import android.test.InstrumentationTestRunner;
import java.io.ByteArrayOutputStream;
import java.io.InputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicReference;

/**
 * Pairs this phone with a real App Server and talks to it through the gateway.
 *
 * Needs a computer: show a code under Settings → Devices and pass its text with
 * {@code -e invite <code>}. Without one the check is skipped, so the suite still
 * runs on a phone that has no computer to reach.
 */
public final class RemoteDeviceChecks extends InstrumentationTestCase {
    public void testPairsWithAnInviteAndReachesTheAppServerThroughTheGateway() throws Exception {
        String invite = ((InstrumentationTestRunner) getInstrumentation()).getArguments().getString("invite");
        if (invite == null) return;
        var context = getInstrumentation().getTargetContext();

        AtomicReference<String> pairingProblem = new AtomicReference<>("pairing did not finish");
        CountDownLatch paired = new CountDownLatch(1);
        RemotePairing.redeem(context, invite, (server, problem) -> { pairingProblem.set(problem); paired.countDown(); });
        assertTrue("Pairing timed out", paired.await(40, TimeUnit.SECONDS));
        assertNull(pairingProblem.get());
        assertTrue(new ConnectionStore(context).usesPaired());

        AtomicReference<String> connectionProblem = new AtomicReference<>("connection did not finish");
        CountDownLatch connected = new CountDownLatch(1);
        RemotePairing.connect(context, problem -> { connectionProblem.set(problem); connected.countDown(); });
        assertTrue("Connection timed out", connected.await(40, TimeUnit.SECONDS));
        assertNull(connectionProblem.get());

        // The bundled Frontend is served to the shell's own WebView and to no one else.
        Response page = request("GET", "/task/task-1", true, null);
        assertEquals(200, page.status);
        assertTrue(page.body.contains("data-shell=\"web\""));
        assertTrue(page.body.contains("data-task-id=\"task-1\""));
        assertEquals(403, request("GET", "/", false, null).status);
        assertEquals(200, request("GET", "/assets/index.js", true, null).status);

        // A request crosses the trusted connection and the App Server answers it.
        Response initialized = request("POST", "/__openaide-app-server/probe", true, initialize(BuildConfig.PROTOCOL_MAJOR, 0));
        assertEquals(initialized.body, 200, initialized.status);
        assertTrue(initialized.body, initialized.body.contains("\"serverId\""));

        // A client newer than the App Server is told to update the computer.
        Response refused = request("POST", "/__openaide-app-server/probe", true, initialize(BuildConfig.PROTOCOL_MAJOR, 9999));
        assertTrue(refused.body, refused.body.contains("incompatibleProtocol"));
    }

    private static String initialize(int major, int minor) {
        return "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"client/initialize\",\"params\":{\"clientInstanceId\":\"device-check-" + minor + "\","
            + "\"shell\":{\"kind\":\"web\"},\"requestedSurface\":{\"kind\":\"home\"},\"capabilities\":{\"protocol\":[],\"shell\":[]},"
            + "\"protocolVersion\":{\"major\":" + major + ",\"minor\":" + minor + "}}}";
    }

    private static final class Response {
        final int status;
        final String body;
        Response(int status, String body) { this.status = status; this.body = body; }
    }

    private static Response request(String method, String path, boolean trusted, String body) throws Exception {
        HttpURLConnection connection = (HttpURLConnection) new URL(ConnectionProfile.paired().endpoint + path.substring(1)).openConnection();
        connection.setConnectTimeout(5000);
        connection.setReadTimeout(30000);
        connection.setRequestMethod(method);
        if (trusted) connection.setRequestProperty("Cookie", GatewayHttp.COOKIE + "=" + WorkspaceGateway.INSTANCE.getToken());
        try {
            if (body != null) {
                connection.setDoOutput(true);
                connection.setRequestProperty("Content-Type", "application/json");
                connection.setRequestProperty("X-OpenAIDE-Connection-Id", "device-check-" + System.nanoTime());
                connection.getOutputStream().write(body.getBytes(StandardCharsets.UTF_8));
            }
            int status = connection.getResponseCode();
            InputStream stream = status >= 400 ? connection.getErrorStream() : connection.getInputStream();
            ByteArrayOutputStream text = new ByteArrayOutputStream();
            if (stream != null) try (stream) { stream.transferTo(text); }
            return new Response(status, text.toString("UTF-8"));
        } finally { connection.disconnect(); }
    }
}
