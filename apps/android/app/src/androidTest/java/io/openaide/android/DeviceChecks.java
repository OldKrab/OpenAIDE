package io.openaide.android;

import android.content.Intent;
import android.net.Uri;
import android.test.InstrumentationTestCase;
import org.json.JSONObject;
import java.io.File;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import java.util.concurrent.atomic.AtomicBoolean;
import java.util.concurrent.atomic.AtomicReference;

public final class DeviceChecks extends InstrumentationTestCase {
    public void testBackOffersTheRendererDismissalBeforeHistory() throws Exception {
        var context = getInstrumentation().getTargetContext();
        MainActivity activity = (MainActivity) getInstrumentation().startActivitySync(new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        try {
            waitForBrowser(activity);
            var field = MainActivity.class.getDeclaredField("browser");
            field.setAccessible(true);
            var page = (android.webkit.WebView) field.get(activity);
            CountDownLatch installed = new CountDownLatch(1);
            getInstrumentation().runOnMainSync(() -> {
                page.resumeTimers();
                page.evaluateJavascript("window.__backHandled=0; window.addEventListener('openaide:back',event=>{event.preventDefault(); window.__backHandled++}, {once:true})", ignored -> installed.countDown());
            });
            assertTrue("Back receiver unavailable", installed.await(10, TimeUnit.SECONDS));
            getInstrumentation().runOnMainSync(activity::onBackPressed);
            CountDownLatch received = new CountDownLatch(1);
            android.os.Handler handler = new android.os.Handler(android.os.Looper.getMainLooper());
            handler.post(new Runnable() {
                @Override public void run() {
                    page.evaluateJavascript("window.__backHandled === 1", value -> {
                        if ("true".equals(value)) received.countDown();
                        else handler.postDelayed(this, 50);
                    });
                }
            });
            try { assertTrue("Native Back skipped the renderer", received.await(10, TimeUnit.SECONDS)); }
            finally { handler.removeCallbacksAndMessages(null); }
        } finally { getInstrumentation().runOnMainSync(activity::finish); }
    }

    public void testRemoteWorkspaceCannotChangeLocalBackgroundProtection() throws Exception {
        var context = getInstrumentation().getTargetContext();
        var activity = getInstrumentation().startActivitySync(new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        var preferences = context.getSharedPreferences("connection", 0);
        boolean original = preferences.getBoolean("background", true);
        AtomicReference<JSONObject> result = new AtomicReference<>();
        getInstrumentation().runOnMainSync(() -> {
            ConnectionController controller = new ConnectionController(activity, new ConnectionController.View() {
                @Override public void render(JSONObject state) { result.set(state); }
                @Override public void changed() { fail("Remote command changed the connection"); }
                @Override public void close() { }
                @Override public void setup(String screen) { }
            }, false);
            try { controller.receive(new JSONObject().put("action", "background").put("enabled", !original)); }
            catch (Exception error) { throw new AssertionError(error); }
            finally { controller.dispose(); }
        });
        assertEquals(original, preferences.getBoolean("background", true));
        assertNotNull(result.get());
        assertTrue(result.get().getString("notice").contains("Switch to this phone"));
        getInstrumentation().runOnMainSync(activity::finish);
    }

    public void testSetupIsOfflineAndCannotLoadExternalContent() throws Exception {
        var context = getInstrumentation().getTargetContext();
        SetupActivity activity = (SetupActivity) getInstrumentation().startActivitySync(
            new Intent(context, SetupActivity.class).putExtra("screen", "welcome").addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        try {
            var field = SetupActivity.class.getDeclaredField("page");
            field.setAccessible(true);
            var page = (android.webkit.WebView) field.get(activity);
            CountDownLatch rendered = new CountDownLatch(1);
            android.os.Handler handler = new android.os.Handler(android.os.Looper.getMainLooper());
            Runnable poll = new Runnable() {
                @Override public void run() {
                    page.evaluateJavascript("!!document.querySelector('[data-screen=local]')", result -> {
                        if ("true".equals(result)) rendered.countDown();
                        else handler.postDelayed(this, 50);
                    });
                }
            };
            handler.post(poll);
            try { assertTrue("Offline setup did not render", rendered.await(10, TimeUnit.SECONDS)); }
            finally { handler.removeCallbacksAndMessages(null); }
            CountDownLatch verified = new CountDownLatch(1);
            AtomicReference<String> result = new AtomicReference<>();
            getInstrumentation().runOnMainSync(() -> page.evaluateJavascript(
                "fetch('https://example.com').then(()=>false,()=>true).then(blocked=>{window.offlineCheck=blocked})", ignored -> {}));
            handler.post(new Runnable() {
                @Override public void run() {
                    page.evaluateJavascript("window.offlineCheck", value -> {
                        if (!"null".equals(value)) { result.set(value); verified.countDown(); }
                        else handler.postDelayed(this, 50);
                    });
                }
            });
            try { assertTrue("Security check did not finish", verified.await(10, TimeUnit.SECONDS)); }
            finally { handler.removeCallbacksAndMessages(null); }
            assertEquals("External content was allowed", "true", result.get());
            getInstrumentation().runOnMainSync(() -> {
                assertFalse(page.getSettings().getAllowFileAccess());
                assertFalse(page.getSettings().getAllowContentAccess());
            });
        } finally { getInstrumentation().runOnMainSync(activity::finish); }
    }

    public void testSelectedImageIsDeliveredThroughTheAndroidResultBoundary() throws Exception {
        var context = getInstrumentation().getTargetContext();
        MainActivity activity = (MainActivity) getInstrumentation().startActivitySync(new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        waitForBrowser(activity);
        CountDownLatch delivered = new CountDownLatch(1);
        AtomicReference<Uri[]> selected = new AtomicReference<>();
        Uri image = Uri.parse("content://" + getInstrumentation().getContext().getPackageName() + ".image/pixel");
        var callback = MainActivity.class.getDeclaredField("fileCallback");
        callback.setAccessible(true);
        getInstrumentation().runOnMainSync(() -> {
            try {
                callback.set(activity, (android.webkit.ValueCallback<Uri[]>) result -> { selected.set(result); delivered.countDown(); });
                activity.onActivityResult(2, android.app.Activity.RESULT_OK, new Intent().setData(image));
            } catch (Exception error) { throw new AssertionError(error); }
        });
        assertTrue("Image result timed out", delivered.await(10, TimeUnit.SECONDS));
        assertNotNull("Selected image was rejected", selected.get());
        assertEquals(image, selected.get()[0]);
        getInstrumentation().runOnMainSync(activity::finish);
    }

    public void testTermuxReturnsStructuredPreflightThroughPendingIntent() throws Exception {
        var context = getInstrumentation().getTargetContext();
        var activity = getInstrumentation().startActivitySync(new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        CountDownLatch completed = new CountDownLatch(1);
        AtomicBoolean success = new AtomicBoolean();
        AtomicReference<String> output = new AtomicReference<>("");
        getInstrumentation().runOnMainSync(() -> TermuxCommand.run(context, "check-termux.sh", ConnectionController.checkEnvironment(), (ok, text) -> {
            success.set(ok);
            output.set(text);
            completed.countDown();
        }));
        assertTrue("Termux callback timed out", completed.await(45, TimeUnit.SECONDS));
        assertTrue("Termux preflight failed", success.get());
        JSONObject checks = new JSONObject(output.get().trim());
        for (String key : new String[]{"runtime", "agent", "agentVersion", "authenticated", "storage"})
            assertTrue("Preflight check: " + key, checks.getBoolean(key));
        // Attaches to the running App Server or launches it, exactly as opening the workspace does.
        assertTrue(LocalServer.status(context).active >= 0);
        getInstrumentation().runOnMainSync(activity::finish);
    }

    public void testIdleReleasesTheActualAndroidWakeLock() {
        var context = getInstrumentation().getTargetContext();
        PowerPolicy policy = new PowerPolicy(0);
        try (WorkProtection protection = new WorkProtection(context.getSystemService(android.os.PowerManager.class), "OpenAIDE:device-test")) {
            policy.observe(1, 0);
            protection.apply(policy.protect(1000, 0));
            assertTrue(protection.isHeld());
            policy.observe(0, 2000);
            protection.apply(policy.protect(2000, 0));
            assertFalse(protection.isHeld());
        }
    }

    public void testNativeStatusReadsTheLocalAppServer() throws Exception {
        var context = getInstrumentation().getTargetContext();
        ServerStatus status = LocalServer.status(context);
        assertTrue(status.active >= 0);
        assertTrue(status.waiting >= 0);
    }

    public void testGatewayRefusesRequestsWithoutTheShellsCookie() throws Exception {
        var context = getInstrumentation().getTargetContext();
        LocalServer.status(context);
        assertTrue(WorkspaceGateway.INSTANCE.start(context, true));
        java.net.HttpURLConnection connection = (java.net.HttpURLConnection)
            new java.net.URL(ConnectionProfile.local().endpoint + "__openaide-app-server/probe").openConnection();
        try { assertEquals(403, connection.getResponseCode()); }
        finally { connection.disconnect(); }
    }

    private void waitForBrowser(MainActivity activity) throws Exception {
        var browser = MainActivity.class.getDeclaredField("browser");
        browser.setAccessible(true);
        CountDownLatch ready = new CountDownLatch(1);
        android.os.Handler handler = new android.os.Handler(android.os.Looper.getMainLooper());
        handler.post(new Runnable() {
            @Override public void run() {
                try {
                    if (browser.get(activity) != null) ready.countDown();
                    else if (!activity.isDestroyed()) handler.postDelayed(this, 50);
                } catch (Exception error) { throw new AssertionError(error); }
            }
        });
        try { assertTrue("Workspace did not open", ready.await(10, TimeUnit.SECONDS)); }
        finally { handler.removeCallbacksAndMessages(null); }
    }
}
