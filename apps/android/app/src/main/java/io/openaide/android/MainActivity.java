package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.ActivityNotFoundException;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.PackageManager;
import android.graphics.Color;
import android.graphics.Typeface;
import android.net.Uri;
import android.os.Bundle;
import android.os.Build;
import android.os.SystemClock;
import android.util.Log;
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
import android.view.ViewGroup;
import java.io.IOException;
import java.io.ByteArrayInputStream;
import java.util.Collections;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/** Android owns connection UI; tasks, credentials and execution stay in Termux or on the paired computer. */
public final class MainActivity extends Activity {
    private static final String PERMISSION = "com.termux.permission.RUN_COMMAND";
    private static final int FILE_REQUEST = 2;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private volatile int generation;
    private ConnectionProfile profile;
    private ConnectionStore connections;
    private boolean visible;
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
    private boolean settingsOpen;
    private boolean credentialsUnavailable;
    private WorkspaceConnectionBridge connectionBridge;
    private boolean pendingSettings;
    private BackNavigation backNavigation;

    @Override public void onCreate(Bundle savedState) {
        super.onCreate(savedState);
        backNavigation = new BackNavigation(this, () -> browser, this::backThroughHistory);
        SharedPreferences preferences = getSharedPreferences("connection", MODE_PRIVATE);
        connections = new ConnectionStore(this);
        try { profile = connections.load(); }
        catch (RuntimeException error) { profile = ConnectionProfile.local(); credentialsUnavailable = true; }
        showConnection();
        if (credentialsUnavailable) openSetup("welcome");
        else if (getIntent().getBooleanExtra("show_settings", false)) showSettings();
        else if (preferences.getBoolean("configured", false) || checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED) requestConnection();
        else openSetup("welcome");
    }

    private void showConnection() {
        generation++;
        connecting = false;
        if (connectionBridge != null) { connectionBridge.dispose(); connectionBridge = null; }
        if (browser != null) { browser.destroy(); browser = null; }
        resourcePolicy.clear();
        LinearLayout layout = new LinearLayout(this);
        layout.setOrientation(LinearLayout.VERTICAL);
        layout.setGravity(Gravity.CENTER);
        int padding = dp(24);
        layout.setPadding(padding, padding * 2, padding, padding);
        layout.setBackgroundColor(shellBackground());
        ImageView icon = new ImageView(this);
        icon.setImageResource(R.mipmap.ic_launcher);
        icon.setContentDescription("OpenAIDE");
        layout.addView(icon, centered(dp(72), dp(72), 0));
        TextView title = new TextView(this);
        title.setText("OpenAIDE");
        title.setTextSize(24);
        title.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        title.setTextColor(shellText());
        title.setGravity(Gravity.CENTER);
        layout.addView(title, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 16));
        TextView instructions = new TextView(this);
        instructions.setText("Your agent workspace");
        instructions.setGravity(Gravity.CENTER);
        instructions.setTextSize(15);
        instructions.setTextColor(shellMuted());
        layout.addView(instructions, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 4));
        status = new TextView(this);
        status.setTextSize(15);
        status.setGravity(Gravity.CENTER);
        status.setTextColor(shellMuted());
        layout.addView(status, centered(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT, 32));
        connect = new Button(this);
        connect.setText("Open workspace");
        connect.setAllCaps(false);
        connect.setTextSize(15);
        connect.setTypeface(Typeface.create("sans-serif-medium", Typeface.NORMAL));
        connect.setTextColor(shellBackground());
        connect.setStateListAnimator(null);
        android.graphics.drawable.GradientDrawable fill = new android.graphics.drawable.GradientDrawable();
        fill.setColor(shellText());
        fill.setCornerRadius(dp(26));
        connect.setBackground(fill);
        connect.setOnClickListener(view -> requestConnection());
        layout.addView(connect, centered(dp(260), dp(52), 28));
        setup = new Button(this);
        setup.setText("Connection settings");
        setup.setAllCaps(false);
        setup.setTextSize(15);
        setup.setTextColor(shellText());
        setup.setStateListAnimator(null);
        setup.setBackgroundColor(Color.TRANSPARENT);
        setup.setOnClickListener(view -> {
            pendingSettings = false;
            openSetup(getSharedPreferences("connection", MODE_PRIVATE).getBoolean("configured", false) ? "settings" : "welcome");
        });
        layout.addView(setup, centered(dp(260), dp(48), 4));
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
        pendingSettings = true;
        if (browser != null) navigateToConnectionSettings();
        else if (getSharedPreferences("connection", MODE_PRIVATE).getBoolean("configured", false)) requestConnection();
        else openSetup("welcome");
    }

    private void navigateToConnectionSettings() {
        if (browser == null) return;
        pendingSettings = false;
        browser.evaluateJavascript("history.pushState(null, '', '/settings?tab=connection'); window.dispatchEvent(new PopStateEvent('popstate'))", null);
    }

    private void openSetup(String screen) {
        if (settingsOpen) return;
        settingsOpen = true;
        startActivityForResult(new Intent(this, SetupActivity.class).putExtra("screen", screen)
            .putExtra("diagnostics", diagnostics.snapshot()), 5);
    }

    private void changeConnection() {
        if (profile.local && browser != null) {
            try { startService(new Intent(this, BackgroundService.class).putExtra("visible", false)); }
            catch (RuntimeException ignored) { }
        }
        profile = connections.load();
        credentialsUnavailable = false;
        showConnection();
        // Forgetting the paired computer leaves no workspace to open until one is chosen.
        if (getSharedPreferences("connection", MODE_PRIVATE).getBoolean("configured", false)) requestConnection();
        else openSetup("welcome");
    }

    private void startBackgroundWork() {
        if (!profile.local) return;
        if (!getSharedPreferences("connection", MODE_PRIVATE).getBoolean("background", true)) return;
        try { startForegroundService(new Intent(this, BackgroundService.class).putExtra("visible", visible)); }
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
        visible = true;
        if (browser != null) {
            browser.resumeTimers();
            browser.onResume();
            if (connectionBridge != null) connectionBridge.resume();
            browser.evaluateJavascript("window.dispatchEvent(new Event('openaide:resume'))", null);
            if (fileCallback == null && !connecting) connect();
        }
    }

    @Override protected void onPause() {
        visible = false;
        if (browser != null && profile.local) startBackgroundWork();
        if (browser != null) { browser.onPause(); browser.pauseTimers(); }
        super.onPause();
    }

    private void requestConnection() {
        if (!profile.local) { connect(); return; }
        try { getPackageManager().getPackageInfo("com.termux", 0); }
        catch (PackageManager.NameNotFoundException error) {
            openSetup("local");
            return;
        }
        if (checkSelfPermission(PERMISSION) != PackageManager.PERMISSION_GRANTED) {
            openSetup("local");
            return;
        }
        connect();
    }

    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        if (connectionBridge != null && SetupPagePolicy.isConnectionRoute(profile, browser.getUrl())) {
            connectionBridge.onRequestPermissionsResult(request, permissions, results);
            return;
        }
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
        if (!profile.local) { connectPaired(attempt, started); return; }
        status.setText("Starting your local workspace…");
        LocalServer.connect(this, problem -> {
            if (attempt != generation || isDestroyed()) return;
            if (problem == null) { opened(attempt, started); return; }
            // A start that fails is usually a phone that is not set up yet, or a runtime
            // older than the app; the check tells those apart from a transient failure.
            TermuxCommand.run(this, "check-termux.sh", ConnectionController.checkEnvironment(), (checked, report) -> {
                if (attempt != generation || isDestroyed()) return;
                boolean ready = checked;
                if (checked) try {
                    org.json.JSONObject checks = new org.json.JSONObject(report.trim());
                    for (String key : new String[]{"node", "nodeVersion", "agent", "agentVersion", "runtime", "storage"}) ready &= checks.optBoolean(key);
                } catch (org.json.JSONException error) { ready = false; }
                if (ready) { fail(attempt, problem); return; }
                fail(attempt, "Let’s finish setting up this phone.");
                openSetup("local");
            });
        });
    }

    /** The workspace's App Server answered; show the workspace or wake the one already shown. */
    private void opened(int attempt, long started) {
        connecting = false;
        getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("configured", true).apply();
        Log.i("OpenAIDE", "connection_end outcome=ready duration_ms=" + (SystemClock.elapsedRealtime() - started));
        diagnostics.record("connection", "ready", attempt, SystemClock.elapsedRealtime() - started);
        startBackgroundWork();
        if (browser == null) showBrowser();
        else if (mainFrameFailed) browser.reload();
        else browser.evaluateJavascript("window.dispatchEvent(new Event('openaide:resume'))", null);
    }

    /** A paired computer needs no probe: reaching its App Server over the trusted connection is the check. */
    private void connectPaired(int attempt, long started) {
        status.setText("Connecting to your computer…");
        RemotePairing.connect(this, problem -> {
            if (attempt != generation || isDestroyed()) return;
            if (problem != null) { fail(attempt, problem); return; }
            opened(attempt, started);
        });
    }

    /** The computer refused this phone while the workspace was open; only the user can repair that. */
    private void pairedConnectionRefused(RemoteNode.Failure failure) {
        runOnUiThread(() -> {
            if (isDestroyed() || profile.local || browser == null) return;
            diagnostics.record("connection", "refused", 0, 0);
            showConnection();
            connect.setText("Try again");
            status.setText(RemotePairing.message(failure));
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
            if (browser != null) Toast.makeText(this, message, Toast.LENGTH_LONG).show();
        });
    }

    @SuppressLint("SetJavaScriptEnabled")
    private void showBrowser() {
        resourcePolicy.use(profile);
        WebView.setWebContentsDebuggingEnabled((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0);
        browser = new WebView(this);
        connectionBridge = new WorkspaceConnectionBridge(this, browser, profile, () -> visible, this::changeConnection, this::openSetup);
        if (visible) browser.resumeTimers();
        else browser.pauseTimers();
        browser.getSettings().setJavaScriptEnabled(true);
        browser.getSettings().setUserAgentString(browser.getSettings().getUserAgentString() + " OpenAIDE-Android/1");
        browser.getSettings().setDomStorageEnabled(true);
        browser.getSettings().setAllowFileAccess(false);
        browser.getSettings().setAllowContentAccess(true);
        browser.setWebViewClient(new WebViewClient() {
            @Override public void onPageStarted(WebView view, String url, android.graphics.Bitmap favicon) {
                mainFrameFailed = false;
                connectionBridge.detach();
            }
            @Override public void onPageFinished(WebView view, String url) {
                if (!profile.owns(url)) return;
                connectionBridge.attach();
                if (pendingSettings) navigateToConnectionSettings();
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
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) {
                Uri uri = request.getUrl();
                if (SetupPagePolicy.opensSettings(profile, view.getUrl(), uri.toString(), request.isForMainFrame(), request.hasGesture())) {
                    showSettings();
                    return true;
                }
                if (profile.owns(uri.toString())) return false;
                if (request.hasGesture() && ("https".equals(uri.getScheme()) || "http".equals(uri.getScheme()))) {
                    try { startActivity(new Intent(Intent.ACTION_VIEW, uri)); }
                    catch (ActivityNotFoundException ignored) { }
                }
                return true;
            }
        });
        browser.setWebChromeClient(new WebChromeClient() {
            @Override public boolean onConsoleMessage(ConsoleMessage message) {
                if (message.messageLevel() == ConsoleMessage.MessageLevel.ERROR || message.messageLevel() == ConsoleMessage.MessageLevel.WARNING)
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
        frame.setBackgroundColor(shellBackground());
        frame.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        frame.addView(browser, new LinearLayout.LayoutParams(-1, 0, 1));
        setContentView(frame);
        // The gateway serves only this WebView: the cookie is its proof on every request.
        android.webkit.CookieManager.getInstance().setCookie(profile.endpoint,
            GatewayHttp.COOKIE + "=" + WorkspaceGateway.INSTANCE.getToken() + "; Path=/; HttpOnly; SameSite=Strict");
        if (!profile.local) WorkspaceGateway.INSTANCE.onFailure(failure -> { pairedConnectionRefused(failure); return kotlin.Unit.INSTANCE; });
        browser.loadUrl(profile.endpoint);
    }

    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        if (request == 4 && connectionBridge != null) {
            connectionBridge.onActivityResult(request, result, data);
            return;
        }
        if (request == 5) {
            settingsOpen = false;
            if (result == RESULT_OK) changeConnection();
            else if (browser == null && !credentialsUnavailable && (getSharedPreferences("connection", MODE_PRIVATE).getBoolean("configured", false)
                || checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED)) requestConnection();
            return;
        }
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
        backNavigation.back();
    }

    private void backThroughHistory() {
        if (browser != null && browser.canGoBack()) browser.goBack();
        else if (browser != null) moveTaskToBack(true);
        else super.onBackPressed();
    }

    private boolean darkTheme() {
        return (getResources().getConfiguration().uiMode & android.content.res.Configuration.UI_MODE_NIGHT_MASK)
            == android.content.res.Configuration.UI_MODE_NIGHT_YES;
    }

    // Neutral colors matching the setup screens: the primary button is text-on-background inverted.
    private int shellBackground() { return darkTheme() ? 0xff1b1d22 : 0xfff6f7f9; }
    private int shellText() { return darkTheme() ? 0xffeceef1 : 0xff23262d; }
    private int shellMuted() { return darkTheme() ? 0xff9da3ae : 0xff666d7a; }

    private int dp(int value) { return (int) (value * getResources().getDisplayMetrics().density); }

    private LinearLayout.LayoutParams centered(int width, int height, int topMarginDp) {
        LinearLayout.LayoutParams params = new LinearLayout.LayoutParams(width, height);
        params.gravity = Gravity.CENTER_HORIZONTAL;
        params.topMargin = dp(topMarginDp);
        return params;
    }

    @Override protected void onDestroy() {
        backNavigation.dispose();
        // TODO: clear only this activity's listener. A recreated activity registers in
        // onCreate before the old one is destroyed, so this can drop the newer listener
        // and a later removal or version mismatch would go unreported until restart.
        WorkspaceGateway.INSTANCE.onFailure(null);
        generation++;
        worker.shutdownNow();
        if (connectionBridge != null) connectionBridge.dispose();
        if (fileCallback != null) {
            diagnostics.record("file_picker", "activity_destroyed", 0, SystemClock.elapsedRealtime() - pickerStarted);
            fileCallback.onReceiveValue(null);
        }
        if (browser != null) browser.destroy();
        resourcePolicy.clear();
        super.onDestroy();
    }
}
