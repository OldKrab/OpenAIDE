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
    private final Activity activity;
    private final AppUpdates updates;
    private Runnable updatesSubscription;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();

    WorkspaceConnectionBridge(Activity activity, WebView browser, ConnectionProfile profile, BooleanSupplier visible, Runnable changed, java.util.function.Consumer<String> setup) {
        this.activity = activity;
        this.browser = browser;
        updates = AppUpdates.get(activity);
        this.profile = profile;
        this.visible = visible;
        appearance = new SystemBarAppearance(activity, browser);
        controller = new ConnectionController(activity, new ConnectionController.View() {
            @Override public void render(JSONObject state) {
                // Every page gets the computers for the switcher; only Settings gets the setup state.
                sendComputers(state.optJSONArray("computers"), state.optBoolean("thisDeviceReady"));
                if (settingsRoute()) send("state", "state", state);
            }
            @Override public void changed() { changed.run(); }
            @Override public void close() { }
            @Override public void setup(String screen) { setup.accept(screen); }
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
                    JSONObject command = request.getJSONObject("command");
                    long started = android.os.SystemClock.elapsedRealtime();
                    diagnostics.record("connection_controls", "started", id, 0);
                    if (!visible.getAsBoolean() || (!settingsRoute() && !SetupPagePolicy.allowedOnEveryPage(command.optString("action")))) {
                        diagnostics.record("connection_controls", "denied", id, android.os.SystemClock.elapsedRealtime() - started);
                        reply(id, false);
                        return;
                    }
                    if (SetupPagePolicy.switchesComputer(command.optString("action"))) {
                        WebMessagePort asked = port;
                        controller.switchComputer(command, (outcome, problem) -> {
                            // A later page numbers its requests anew; only the page that asked is answered.
                            if (port == asked) answer(id, outcome, problem);
                        });
                        diagnostics.record("connection_controls", "switch_requested", id, android.os.SystemClock.elapsedRealtime() - started);
                        return;
                    }
                    if (command.optString("action").startsWith("update_")) updates.command(activity, command);
                    else controller.receive(command);
                    diagnostics.record("connection_controls", "accepted", id, android.os.SystemClock.elapsedRealtime() - started);
                    reply(id, true);
                } catch (Exception error) { diagnostics.record("connection_controls", "invalid_request", 0, 0); }
            }
        }, new Handler(Looper.getMainLooper()));
        browser.postWebMessage(new WebMessage("openaide:connection-controls:1", new WebMessagePort[]{channel[1]}),
            Uri.parse(profile.endpoint.substring(0, profile.endpoint.length() - 1)));
        if (settingsRoute()) controller.resume();
        else controller.refresh();
        // Every workspace page gets the update state, so navigation can mark an available update.
        updatesSubscription = updates.subscribe(state -> send("update", "update", state));
    }

    private boolean settingsRoute() { return SetupPagePolicy.isConnectionRoute(profile, browser.getUrl()); }

    private void send(String type, String key, Object value) {
        if (port == null) return;
        try { port.postMessage(new WebMessage(new JSONObject().put("type", type).put(key, value).toString())); }
        catch (Exception ignored) { }
    }

    /** The switcher's list, and whether it may offer this phone (ADR-0070). */
    private void sendComputers(org.json.JSONArray computers, boolean thisDeviceReady) {
        if (port == null) return;
        try {
            port.postMessage(new WebMessage(new JSONObject().put("type", "computers").put("computers", computers)
                .put("thisDeviceReady", thisDeviceReady).toString()));
        } catch (Exception ignored) { }
    }

    private void reply(int id, boolean accepted) {
        if (port == null) return;
        try { port.postMessage(new WebMessage(new JSONObject().put("type", "result").put("id", id).put("accepted", accepted).toString())); }
        catch (Exception ignored) { }
    }

    /** The end of a switch: its outcome, or the sentence saying why nothing changed. */
    private void answer(int id, String outcome, String problem) {
        if (port == null) return;
        try {
            JSONObject result = new JSONObject().put("type", "result").put("id", id).put("accepted", problem == null);
            if (problem == null) result.put("outcome", outcome);
            else result.put("error", problem);
            port.postMessage(new WebMessage(result.toString()));
        } catch (Exception ignored) { }
    }

    /**
     * Makes this phone the workspace in use through the connect-first switch, as the
     * switcher does, for a tap on one of its Task notifications; [reply] answers once.
     */
    void useThisPhone(String operationId, ComputerSwitch.Reply reply) {
        try { controller.switchComputer(new JSONObject().put("action", "local").put("operationId", operationId), reply); }
        catch (org.json.JSONException impossible) { reply.done(null, "OpenAIDE could not switch. Try again."); }
    }

    void resume() {
        if (settingsRoute()) controller.resume();
        else controller.refresh();
    }
    void onRequestPermissionsResult(int request, String[] permissions, int[] results) { controller.onRequestPermissionsResult(request, permissions, results); }
    void onActivityResult(int request, int result, Intent data) { controller.onActivityResult(request, result, data); }
    void detach() {
        if (updatesSubscription != null) { updatesSubscription.run(); updatesSubscription = null; }
        if (port != null) { port.close(); port = null; }
    }
    void dispose() { detach(); controller.dispose(); }
}
