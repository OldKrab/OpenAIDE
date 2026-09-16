package io.openaide.android;

import android.app.Activity;
import android.content.Intent;
import android.net.Uri;
import android.os.Handler;
import android.os.Looper;
import android.webkit.WebMessage;
import android.webkit.WebMessagePort;
import android.webkit.WebView;
import java.util.function.BooleanSupplier;
import org.json.JSONObject;

final class WorkspaceConnectionBridge {
    private final WebView browser;
    private final ConnectionProfile profile;
    private final BooleanSupplier visible;
    private final ConnectionController controller;
    private final SystemBarAppearance appearance;
    private WebMessagePort port;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();

    WorkspaceConnectionBridge(Activity activity, WebView browser, ConnectionProfile profile, BooleanSupplier visible, Runnable changed) {
        this.browser = browser;
        this.profile = profile;
        this.visible = visible;
        appearance = new SystemBarAppearance(activity, browser);
        controller = new ConnectionController(activity, new ConnectionController.View() {
            @Override public void render(JSONObject state) { if (settingsRoute()) send("state", "state", state); }
            @Override public void changed() { changed.run(); }
            @Override public void close() { }
            @Override public void scanned(String address) { if (settingsRoute()) send("scanned", "address", address); }
        }, profile.local);
    }

    void attach() {
        detach();
        if (!profile.owns(browser.getUrl())) return;
        WebMessagePort[] channel = browser.createWebMessageChannel();
        port = channel[0];
        port.setWebMessageCallback(new WebMessagePort.WebMessageCallback() {
            @Override public void onMessage(WebMessagePort source, WebMessage message) {
                if (source != port || message.getData() == null || message.getData().length() > 16384) return;
                try {
                    JSONObject request = new JSONObject(message.getData());
                    if ("appearance".equals(request.optString("type"))) {
                        if (profile.owns(browser.getUrl())) appearance.apply(request.optString("color", null));
                        return;
                    }
                    int id = request.getInt("id");
                    long started = android.os.SystemClock.elapsedRealtime();
                    diagnostics.record("connection_controls", "started", id, 0);
                    if (!visible.getAsBoolean() || !settingsRoute()) {
                        diagnostics.record("connection_controls", "denied", id, android.os.SystemClock.elapsedRealtime() - started);
                        reply(id, false);
                        return;
                    }
                    controller.receive(request.getJSONObject("command"));
                    diagnostics.record("connection_controls", "accepted", id, android.os.SystemClock.elapsedRealtime() - started);
                    reply(id, true);
                } catch (Exception error) { diagnostics.record("connection_controls", "invalid_request", 0, 0); }
            }
        }, new Handler(Looper.getMainLooper()));
        browser.postWebMessage(new WebMessage("openaide:connection-controls:1", new WebMessagePort[]{channel[1]}),
            Uri.parse(profile.endpoint.substring(0, profile.endpoint.length() - 1)));
        if (settingsRoute()) controller.resume();
    }

    private boolean settingsRoute() { return SetupPagePolicy.isConnectionRoute(profile, browser.getUrl()); }

    private void send(String type, String key, Object value) {
        if (port == null) return;
        try { port.postMessage(new WebMessage(new JSONObject().put("type", type).put(key, value).toString())); }
        catch (Exception ignored) { }
    }

    private void reply(int id, boolean accepted) {
        if (port == null) return;
        try { port.postMessage(new WebMessage(new JSONObject().put("type", "result").put("id", id).put("accepted", accepted).toString())); }
        catch (Exception ignored) { }
    }

    void resume() { if (settingsRoute()) controller.resume(); }
    void onRequestPermissionsResult(int request, String[] permissions, int[] results) { controller.onRequestPermissionsResult(request, permissions, results); }
    void onActivityResult(int request, int result, Intent data) { controller.onActivityResult(request, result, data); }
    void detach() {
        if (port != null) { port.close(); port = null; }
    }
    void dispose() { detach(); controller.dispose(); }
}
