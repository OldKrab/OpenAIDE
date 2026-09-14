package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.os.SystemClock;
import android.provider.Settings;
import android.util.Base64;
import android.util.Log;
import android.webkit.HttpAuthHandler;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import java.io.IOException;
import java.io.ByteArrayOutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.SecureRandom;
import java.util.Collections;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Android owns connection UI; all tasks, credentials and execution stay in Termux. */
public final class MainActivity extends Activity {
    private static final String PERMISSION = "com.termux.permission.RUN_COMMAND";
    private static final String ENDPOINT = "http://127.0.0.1:5474/";
    private static final int FILE_REQUEST = 2;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private volatile int generation;
    private String password;
    private TextView status;
    private Button connect;
    private WebView browser;
    private ValueCallback<Uri[]> fileCallback;

    @Override public void onCreate(Bundle savedState) {
        super.onCreate(savedState);
        SharedPreferences preferences = getSharedPreferences("connection", MODE_PRIVATE);
        password = preferences.getString("password", null);
        if (password == null) {
            byte[] random = new byte[32];
            new SecureRandom().nextBytes(random);
            StringBuilder encoded = new StringBuilder();
            for (byte value : random) encoded.append(String.format("%02x", value & 255));
            password = encoded.toString();
            preferences.edit().putString("password", password).apply();
        }
        showConnection();
    }

    private void showConnection() {
        generation++;
        if (browser != null) { browser.destroy(); browser = null; }
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        int padding = (int) (24 * getResources().getDisplayMetrics().density);
        layout.setPadding(padding, padding * 2, padding, padding);
        layout.setBackgroundColor(Color.rgb(245, 246, 248));
        TextView title = new TextView(this);
        title.setText("OpenAIDE");
        title.setTextSize(30);
        layout.addView(title);
        TextView instructions = new TextView(this);
        instructions.setText("Run your agents on this phone through Termux.\n\n"
            + "One-time setup: install the OpenAIDE Termux runtime, Node.js, Git and Codex. "
            + "In ~/.termux/termux.properties enable allow-external-apps=true.\n\n"
            + "Grant OpenAIDE permission to run commands in Termux when prompted. "
            + "Your projects and Codex login remain in Termux.\n");
        instructions.setTextSize(16);
        layout.addView(instructions);
        connect = new Button(this);
        connect.setText("Connect to Termux");
        connect.setOnClickListener(view -> requestConnection());
        layout.addView(connect);
        Button permissions = new Button(this);
        permissions.setText("App permissions");
        permissions.setOnClickListener(view -> startActivity(new Intent(
            Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + getPackageName()))));
        layout.addView(permissions);
        status = new TextView(this);
        status.setTextSize(16);
        layout.addView(status);
        setContentView(layout);
    }

    private void requestConnection() {
        try { getPackageManager().getPackageInfo("com.termux", 0); }
        catch (PackageManager.NameNotFoundException error) {
            status.setText("Install and open Termux first, then return here.");
            return;
        }
        if (checkSelfPermission(PERMISSION) != PackageManager.PERMISSION_GRANTED) {
            requestPermissions(new String[]{PERMISSION}, 1);
            return;
        }
        connect();
    }

    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (request == 1 && results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) connect();
        else status.setText("Open App permissions and allow running commands in Termux, then connect again.");
    }

    private void connect() {
        final int attempt = ++generation;
        final long started = SystemClock.elapsedRealtime();
        connect.setEnabled(false);
        status.setText("Connecting to OpenAIDE…");
        Log.i("OpenAIDE", "connection_start attempt=" + attempt);
        worker.execute(() -> {
            int response = probe();
            if (response == 200) { finishConnection(attempt, started, true); return; }
            if (response == 503) { waitForServer(attempt, started); return; }
            if (response == 401) {
                fail(attempt, "A server with different credentials uses port 5474. Stop it in Termux, then reconnect.");
                return;
            }
            runOnUiThread(() -> {
                if (attempt != generation) return;
                try {
                    String script;
                    try (var input = getAssets().open("start-termux.sh")) {
                        ByteArrayOutputStream output = new ByteArrayOutputStream();
                        byte[] buffer = new byte[4096];
                        int count;
                        while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
                        script = output.toString(StandardCharsets.UTF_8.name());
                    }
                    Intent command = new Intent("com.termux.RUN_COMMAND");
                    command.setClassName("com.termux", "com.termux.app.RunCommandService");
                    command.putExtra("com.termux.RUN_COMMAND_PATH", "/data/data/com.termux/files/usr/bin/bash");
                    command.putExtra("com.termux.RUN_COMMAND_ARGUMENTS", new String[]{"-s"});
                    command.putExtra("com.termux.RUN_COMMAND_STDIN",
                        "export OPENAIDE_WEB_PASSWORD='" + password + "'\n" + script);
                    command.putExtra("com.termux.RUN_COMMAND_BACKGROUND", true);
                    startService(command);
                    waitForServer(attempt, started);
                } catch (IOException | RuntimeException error) {
                    fail(attempt, "Could not start Termux. Check its installation and command permission.");
                }
            });
        });
    }

    private void waitForServer(int attempt, long started) {
        worker.execute(() -> {
            while (attempt == generation && SystemClock.elapsedRealtime() - started < 45_000) {
                if (probe() == 200) { finishConnection(attempt, started, true); return; }
                try { Thread.sleep(500); }
                catch (InterruptedException error) { Thread.currentThread().interrupt(); return; }
            }
            if (attempt == generation) fail(attempt,
                "OpenAIDE did not start. Check allow-external-apps=true and the runtime installation. "
                + "Details: ~/.local/share/openaide-android/state/launcher.log in Termux.");
        });
    }

    private int probe() {
        int authorized = requestStatus(ENDPOINT, true);
        if (authorized != 200) return authorized;
        if (requestStatus(ENDPOINT, false) != 401) return 401;
        return requestStatus(ENDPOINT + "readyz", true);
    }

    private int requestStatus(String address, boolean authenticated) {
        HttpURLConnection connection = null;
        try {
            connection = (HttpURLConnection) new URL(address).openConnection();
            connection.setConnectTimeout(1500);
            connection.setReadTimeout(1500);
            connection.setInstanceFollowRedirects(false);
            if (authenticated) connection.setRequestProperty("Authorization", authorization());
            return connection.getResponseCode();
        } catch (IOException error) { return -1; }
        finally { if (connection != null) connection.disconnect(); }
    }

    private String authorization() {
        return "Basic " + Base64.encodeToString(("android:" + password).getBytes(StandardCharsets.UTF_8), Base64.NO_WRAP);
    }

    private void finishConnection(int attempt, long started, boolean ready) {
        runOnUiThread(() -> {
            if (attempt != generation) return;
            Log.i("OpenAIDE", "connection_end outcome=ready duration_ms=" + (SystemClock.elapsedRealtime() - started));
            showBrowser();
        });
    }

    private void fail(int attempt, String message) {
        runOnUiThread(() -> {
            if (attempt != generation) return;
            Log.w("OpenAIDE", "connection_end outcome=failed attempt=" + attempt);
            connect.setEnabled(true);
            status.setText(message);
        });
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void showBrowser() {
        browser = new WebView(this);
        browser.getSettings().setJavaScriptEnabled(true);
        browser.getSettings().setDomStorageEnabled(true);
        browser.getSettings().setAllowFileAccess(false);
        browser.getSettings().setAllowContentAccess(true);
        browser.setWebViewClient(new WebViewClient() {
            @Override public void onReceivedHttpAuthRequest(WebView view, HttpAuthHandler handler, String host, String realm) {
                if ("127.0.0.1".equals(host) && "OpenAIDE".equals(realm)) handler.proceed("android", password);
                else handler.cancel();
            }
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                if ("http".equals(uri.getScheme()) && "127.0.0.1".equals(uri.getHost()) && uri.getPort() == 5474) return false;
                if (request.hasGesture() && ("https".equals(uri.getScheme()) || "http".equals(uri.getScheme()))) {
                    try { startActivity(new Intent(Intent.ACTION_VIEW, uri)); }
                    catch (ActivityNotFoundException ignored) { }
                }
                return true;
            }
        });
        browser.setWebChromeClient(new WebChromeClient() {
            @Override public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                if (fileCallback != null) fileCallback.onReceiveValue(null);
                fileCallback = callback;
                try { startActivityForResult(params.createIntent(), FILE_REQUEST); }
                catch (ActivityNotFoundException error) { fileCallback.onReceiveValue(null); fileCallback = null; }
                return true;
            }
        });
        LinearLayout frame = new LinearLayout(this);
        frame.setOrientation(LinearLayout.VERTICAL);
        frame.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        Button connectionButton = new Button(this);
        connectionButton.setText("Connection");
        connectionButton.setOnClickListener(view -> showConnection());
        frame.addView(connectionButton);
        frame.addView(browser, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(frame);
        browser.loadUrl(ENDPOINT, Collections.singletonMap("Authorization", authorization()));
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == FILE_REQUEST && fileCallback != null) {
            fileCallback.onReceiveValue(WebChromeClient.FileChooserParams.parseResult(result, data));
            fileCallback = null;
        }
    }

    @Override public void onBackPressed() {
        if (browser != null && browser.canGoBack()) browser.goBack();
        else if (browser != null) showConnection();
        else super.onBackPressed();
    }

    @Override protected void onDestroy() {
        generation++;
        worker.shutdownNow();
        if (fileCallback != null) fileCallback.onReceiveValue(null);
        if (browser != null) browser.destroy();
        super.onDestroy();
    }
}
