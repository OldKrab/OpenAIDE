package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.app.AlertDialog;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.net.Uri;
import android.os.Bundle;
import android.os.Build;
import android.os.SystemClock;
import android.provider.Settings;
import android.util.Base64;
import android.util.Log;
import android.webkit.HttpAuthHandler;
import android.webkit.ValueCallback;
import android.webkit.WebChromeClient;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebResourceError;
import android.webkit.ConsoleMessage;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.Button;
import android.widget.LinearLayout;
import android.widget.TextView;
import android.widget.ScrollView;
import android.widget.ImageView;
import android.widget.Toast;
import android.view.Gravity;
import android.view.View;
import java.io.IOException;
import java.io.ByteArrayOutputStream;
import java.io.ByteArrayInputStream;
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
    private Button setup;
    private WebView browser;
    private ValueCallback<Uri[]> fileCallback;
    private long pickerStarted;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private final WebResourcePolicy resourcePolicy = new WebResourcePolicy();
    private boolean connecting;
    private boolean mainFrameFailed;

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
        if (checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED) requestConnection();
        if (getIntent().getBooleanExtra("show_settings", false)) showSettings();
    }

    private void showConnection() {
        generation++;
        if (browser != null) { browser.destroy(); browser = null; }
        resourcePolicy.clear();
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setGravity(Gravity.CENTER);
        int padding = (int) (24 * getResources().getDisplayMetrics().density);
        layout.setPadding(padding, padding * 2, padding, padding);
        layout.setBackgroundColor(Color.rgb(245, 246, 248));
        ImageView icon = new ImageView(this);
        icon.setImageResource(R.mipmap.ic_launcher);
        icon.setContentDescription("OpenAIDE");
        layout.addView(icon, new LinearLayout.LayoutParams(padding * 4, padding * 4));
        TextView title = new TextView(this);
        title.setText("OpenAIDE");
        title.setTextSize(30);
        layout.addView(title);
        TextView instructions = new TextView(this);
        instructions.setText("Your workspace, on this phone.\n\n"
            + "Code with your agents. Your projects and login stay in Termux.\n"
            + "Background mode lets work continue with the screen locked.\n");
        instructions.setGravity(Gravity.CENTER);
        instructions.setTextSize(16);
        layout.addView(instructions);
        connect = new Button(this);
        connect.setText("Start working");
        connect.setOnClickListener(view -> requestConnection());
        layout.addView(connect);
        setup = new Button(this);
        setup.setText("Setup & background");
        setup.setOnClickListener(view -> showSettings());
        layout.addView(setup);
        if (checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED) {
            instructions.setVisibility(View.GONE);
            setup.setVisibility(View.GONE);
        }
        status = new TextView(this);
        status.setTextSize(16);
        layout.addView(status);
        ScrollView scroll = new ScrollView(this);
        scroll.setFillViewport(true);
        scroll.addView(layout);
        scroll.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        setContentView(scroll);
    }

    private void showSettings() {
        new AlertDialog.Builder(this).setTitle("On-device workspace")
            .setItems(new String[]{"Background mode", "Battery settings", "Termux settings", "App permissions", "Setup help", "Share diagnostics"},
                (dialog, which) -> {
                    if (which == 0) {
                        boolean enabled = getSharedPreferences("connection", MODE_PRIVATE).getBoolean("background", true);
                        new AlertDialog.Builder(this).setTitle("Work with the screen locked")
                            .setMessage("Keeps the CPU awake and shows a notification. This uses extra battery. "
                                + "Allow unrestricted battery use for both Termux and OpenAIDE for reliable background work.")
                            .setPositiveButton(enabled ? "Turn off" : "Enable", (choice, button) -> {
                                getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("background", !enabled).apply();
                                if (enabled) stopService(new Intent(this, BackgroundService.class));
                                else startBackgroundWork();
                            }).setNegativeButton("Cancel", null).show();
                    } else if (which == 1) openSettingsIntent(new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS));
                    else if (which == 2) openSettingsIntent(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:com.termux")));
                    else if (which == 3) openSettingsIntent(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + getPackageName())));
                    else if (which == 4) new AlertDialog.Builder(this).setTitle("One-time setup")
                        .setMessage("Install Termux, Node.js, Git, Codex and the OpenAIDE runtime. "
                            + "Sign in to Codex in Termux. Enable allow-external-apps=true in ~/.termux/termux.properties, "
                            + "then grant OpenAIDE permission to run commands. Future launches connect automatically.")
                        .setPositiveButton("Done", null).show();
                    else {
                        Intent share = new Intent(Intent.ACTION_SEND).setType("text/plain");
                        share.putExtra(Intent.EXTRA_TEXT, diagnostics.snapshot());
                        startActivity(Intent.createChooser(share, "Share OpenAIDE diagnostics"));
                    }
                }).setPositiveButton("Done", null).show();
    }

    private void openSettingsIntent(Intent intent) {
        try { startActivity(intent); }
        catch (ActivityNotFoundException error) {
            Toast.makeText(this, "Open your phone's Settings app to change this option.", Toast.LENGTH_LONG).show();
        }
    }

    private void startBackgroundWork() {
        if (!getSharedPreferences("connection", MODE_PRIVATE).getBoolean("background", true)) return;
        try { startForegroundService(new Intent(this, BackgroundService.class)); }
        catch (RuntimeException error) {
            diagnostics.record("background_work", "start_failed", 0, 0);
            Toast.makeText(this, "Background mode could not start. Keep OpenAIDE open and try again.", Toast.LENGTH_LONG).show();
        }
        if (Build.VERSION.SDK_INT >= 33 && checkSelfPermission("android.permission.POST_NOTIFICATIONS") != PackageManager.PERMISSION_GRANTED
            && !getSharedPreferences("connection", MODE_PRIVATE).getBoolean("notification_requested", false)) {
            getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("notification_requested", true).apply();
            requestPermissions(new String[]{"android.permission.POST_NOTIFICATIONS"}, 3);
        }
    }

    @Override protected void onNewIntent(Intent intent) {
        super.onNewIntent(intent);
        setIntent(intent);
        if (intent.getBooleanExtra("show_settings", false)) showSettings();
    }

    @Override protected void onResume() {
        super.onResume();
        if (browser != null) {
            browser.onResume();
            browser.evaluateJavascript("window.dispatchEvent(new Event('openaide:resume'))", null);
            if (fileCallback == null && !connecting) connect();
        }
    }

    @Override protected void onPause() {
        if (browser != null) browser.onPause();
        super.onPause();
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
        if (request != 1) return;
        if (request == 1 && results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) connect();
        else status.setText("Open App permissions and allow running commands in Termux, then connect again.");
    }

    private void connect() {
        if (connecting) return;
        connecting = true;
        final int attempt = ++generation;
        final long started = SystemClock.elapsedRealtime();
        connect.setEnabled(false);
        connect.setVisibility(View.GONE);
        status.setText("Opening your workspace…");
        Log.i("OpenAIDE", "connection_start attempt=" + attempt);
        worker.execute(() -> {
            int response = probe();
            if (response == 200) { finishConnection(attempt, started); return; }
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
                if (probe() == 200) { finishConnection(attempt, started); return; }
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

    private void finishConnection(int attempt, long started) {
        runOnUiThread(() -> {
            if (attempt != generation) return;
            connecting = false;
            Log.i("OpenAIDE", "connection_end outcome=ready duration_ms=" + (SystemClock.elapsedRealtime() - started));
            diagnostics.record("connection", "ready", attempt, SystemClock.elapsedRealtime() - started);
            startBackgroundWork();
            if (browser == null) showBrowser();
            else if (mainFrameFailed) browser.reload();
            else browser.evaluateJavascript("window.dispatchEvent(new Event('openaide:resume'))", null);
        });
    }

    private void fail(int attempt, String message) {
        runOnUiThread(() -> {
            if (attempt != generation) return;
            connecting = false;
            Log.w("OpenAIDE", "connection_end outcome=failed attempt=" + attempt);
            connect.setEnabled(true);
            connect.setVisibility(View.VISIBLE);
            setup.setVisibility(View.VISIBLE);
            connect.setText("Try again");
            status.setText(message);
            diagnostics.record("connection", "failed", attempt, 0);
            if (browser != null) new AlertDialog.Builder(this).setTitle("Connection interrupted")
                .setMessage(message).setPositiveButton("Retry", (dialog, which) -> connect())
                .setNeutralButton("Settings", (dialog, which) -> showSettings())
                .setNegativeButton("Later", null).show();
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
            @Override public void onPageStarted(WebView view, String url, android.graphics.Bitmap favicon) {
                mainFrameFailed = false;
            }
            @Override public void onReceivedError(WebView view, WebResourceRequest request, WebResourceError error) {
                if (request.isForMainFrame()) mainFrameFailed = true;
                diagnostics.record("web_resource", "failed", error.getErrorCode(), 0);
            }
            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                if (resourcePolicy.allows(uri.toString())) return null;
                diagnostics.record("web_resource", "blocked", 0, 0);
                return new WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden",
                    Collections.emptyMap(), new ByteArrayInputStream(new byte[0]));
            }
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
            @Override public boolean onConsoleMessage(ConsoleMessage message) {
                diagnostics.record("web_console", message.messageLevel().name(), message.lineNumber(), 0);
                return true;
            }
            @Override public boolean onShowFileChooser(WebView view, ValueCallback<Uri[]> callback, FileChooserParams params) {
                if (fileCallback != null) {
                    diagnostics.record("file_picker", "replaced", 0, SystemClock.elapsedRealtime() - pickerStarted);
                    fileCallback.onReceiveValue(null);
                }
                fileCallback = callback;
                pickerStarted = SystemClock.elapsedRealtime();
                diagnostics.record("file_picker", "started", 0, 0);
                try {
                    Intent picker = new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE);
                    String[] types = params.getAcceptTypes();
                    picker.setType(types.length == 1 && types[0].contains("/") ? types[0] : "*/*");
                    if (types.length > 1) picker.putExtra(Intent.EXTRA_MIME_TYPES, types);
                    picker.putExtra(Intent.EXTRA_ALLOW_MULTIPLE, params.getMode() == FileChooserParams.MODE_OPEN_MULTIPLE);
                    picker.addFlags(Intent.FLAG_GRANT_READ_URI_PERMISSION | Intent.FLAG_GRANT_PERSISTABLE_URI_PERMISSION);
                    startActivityForResult(picker, FILE_REQUEST);
                }
                catch (ActivityNotFoundException error) {
                    diagnostics.record("file_picker", "unavailable", 0, SystemClock.elapsedRealtime() - pickerStarted);
                    fileCallback.onReceiveValue(null);
                    fileCallback = null;
                }
                return true;
            }
        });
        LinearLayout frame = new LinearLayout(this);
        frame.setOrientation(LinearLayout.VERTICAL);
        frame.setBackgroundColor(Color.rgb(248, 249, 251));
        frame.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        frame.addView(browser, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(frame);
        browser.loadUrl(ENDPOINT, Collections.singletonMap("Authorization", authorization()));
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == FILE_REQUEST && fileCallback != null) {
            Uri[] selected = WebChromeClient.FileChooserParams.parseResult(result, data);
            diagnostics.record("file_picker", selected == null ? "cancelled" : "selected",
                selected == null ? 0 : selected.length, SystemClock.elapsedRealtime() - pickerStarted);
            ValueCallback<Uri[]> callback = fileCallback;
            fileCallback = null;
            if (selected == null) { callback.onReceiveValue(null); return; }
            WebView owner = browser;
            worker.execute(() -> {
                boolean readable = true;
                try {
                    for (Uri uri : selected) {
                        if (!"content".equals(uri.getScheme())) throw new IOException("Unsupported document scheme");
                        try (var document = getContentResolver().openAssetFileDescriptor(uri, "r")) {
                            if (document == null) throw new IOException("Unreadable document");
                        }
                    }
                } catch (IOException | RuntimeException error) { readable = false; }
                final boolean accepted = readable;
                runOnUiThread(() -> {
                    if (isDestroyed() || owner != browser) { callback.onReceiveValue(null); return; }
                    if (accepted) {
                        for (Uri uri : selected) resourcePolicy.allowDocument(uri.toString());
                        callback.onReceiveValue(selected);
                        browser.evaluateJavascript("window.dispatchEvent(new Event('openaide:resume'))", null);
                    } else {
                        callback.onReceiveValue(null);
                        Toast.makeText(this, "Could not read that file. Download it to this phone and choose it again.", Toast.LENGTH_LONG).show();
                    }
                    diagnostics.record("file_picker", accepted ? "delivered" : "unreadable", selected.length,
                        SystemClock.elapsedRealtime() - pickerStarted);
                });
            });
        } else if (request == FILE_REQUEST) {
            diagnostics.record("file_picker", "callback_lost", 0, 0);
        }
    }

    @Override public void onBackPressed() {
        if (browser != null && browser.canGoBack()) browser.goBack();
        else if (browser != null) moveTaskToBack(true);
        else super.onBackPressed();
    }

    @Override protected void onDestroy() {
        generation++;
        worker.shutdownNow();
        if (fileCallback != null) {
            diagnostics.record("file_picker", "activity_destroyed", 0, SystemClock.elapsedRealtime() - pickerStarted);
            fileCallback.onReceiveValue(null);
        }
        if (browser != null) browser.destroy();
        resourcePolicy.clear();
        super.onDestroy();
    }
}
