package io.openaide.android;

import android.app.Activity;
import android.content.ClipData;
import android.content.ClipboardManager;
import android.content.ContextWrapper;
import android.content.Intent;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.PowerManager;
import android.os.SystemClock;
import android.provider.Settings;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import org.json.JSONObject;

final class ConnectionController extends ContextWrapper {
    interface View {
        void render(JSONObject state);
        void changed();
        void close();
        /** Opens the full-screen setup, which owns pairing because it must work with no workspace. */
        void setup(String screen);
    }
    private static final String PERMISSION = "com.termux.permission.RUN_COMMAND";
    static final int SCAN_REQUEST = 4;
    private final Activity activity;
    private final View view;
    private final boolean localTools;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private JSONObject checks;
    private boolean busy;
    private boolean disposed;
    private boolean recheckOnReturn;
    private String notice = "";
    private long operationStarted;
    private int operationSequence;
    private String operationName;
    private JSONObject joinCode;

    ConnectionController(Activity activity, View view, boolean localTools) {
        super(activity);
        this.activity = activity;
        this.view = view;
        this.localTools = localTools;
    }

    void receive(JSONObject message) {
        if (disposed) return;
        String command = message.optString("action");
        if (!localTools && !java.util.Arrays.asList("state", "close", "local", "paired", "pair_setup", "forget", "app_settings", "diagnostics").contains(command)) {
            notice = "Switch to this phone before managing local tools.";
            emit();
            return;
        }
        try { action(message); }
        catch (Exception error) { notice = "That action could not finish. Please try again."; emit(); }
    }

    private void action(JSONObject message) throws Exception {
        String action = message.optString("action");
        if (action.equals("state")) { emit(); return; }
        if (busy && !action.equals("close")) return;
        notice = "";
        switch (action) {
            case "close": view.close(); break;
            case "state": emit(); break;
            case "check": check(); break;
            case "local": new ConnectionStore(this).selectLocal(); changed(); break;
            case "paired": new ConnectionStore(this).selectPaired(); changed(); break;
            case "pair_setup": view.setup("remote"); break;
            case "scan": scan(); break;
            case "pair": pair(message.optString("code")); break;
            case "join": join(); break;
            case "join_stop": RemotePairing.stopJoin(); joinCode = null; emit(); break;
            case "forget": RemotePairing.forget(this); changed(); break;
            case "install": install(); break;
            case "termux": launchTermux(); break;
            case "get_termux": open(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/termux/termux-app/releases"))); break;
            case "access":
                copy("mkdir -p ~/.termux; printf '\\nallow-external-apps=true\\n' >> ~/.termux/termux.properties; termux-reload-settings");
                recheckOnReturn = true;
                launchTermux();
                break;
            case "grant": requestPermissions(new String[]{PERMISSION}, 1); break;
            case "signin":
                // The check names the command of the agent that is installed: Codex or Claude.
                String signin = checks == null ? "" : checks.optString("signin");
                copy("claude".equals(signin) ? "claude" : "codex login");
                recheckOnReturn = true;
                launchTermux();
                break;
            case "battery": open(new Intent(Settings.ACTION_IGNORE_BATTERY_OPTIMIZATION_SETTINGS)); break;
            case "termux_settings": open(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:com.termux"))); break;
            case "app_settings": open(new Intent(Settings.ACTION_APPLICATION_DETAILS_SETTINGS, Uri.parse("package:" + getPackageName()))); break;
            case "background":
                boolean enabled = message.optBoolean("enabled", true);
                getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("background", enabled).apply();
                if (!enabled) stopService(new Intent(this, BackgroundService.class));
                else startForegroundService(new Intent(this, BackgroundService.class).putExtra("visible", true));
                emit();
                break;
            case "diagnostics":
                open(Intent.createChooser(new Intent(Intent.ACTION_SEND).setType("text/plain").putExtra(Intent.EXTRA_TEXT,
                    diagnostics.snapshot()), "Share diagnostics"));
                break;
            default: break;
        }
    }

    private void check() {
        if (!installed("com.termux") || checkSelfPermission(PERMISSION) != PackageManager.PERMISSION_GRANTED) { emit(); return; }
        begin("setup_preflight", "Checking your phone…");
        TermuxCommand.run(this, "check-termux.sh", checkEnvironment(), (success, output) -> {
            if (isDestroyed()) return;
            checks = null;
            if (success) try { checks = new JSONObject(output.trim()); } catch (Exception ignored) { success = false; }
            end(success, success ? "" : "Termux needs permission to connect. Complete the one-time connection step below.");
        });
    }

    /** The runtime in Termux must be the app's own version; the check compares against it. */
    static String checkEnvironment() { return TermuxCommand.variable("OPENAIDE_VERSION", BuildConfig.VERSION_NAME); }

    private void install() {
        if (checks == null) { check(); return; }
        boolean needsRuntime = !checks.optBoolean("runtime");
        begin("setup_install", "Preparing your workspace download…");
        worker.execute(() -> {
            String environment;
            try {
                environment = (needsRuntime ? RuntimeRelease.installEnvironment() : "")
                    + TermuxCommand.variable("OPENAIDE_CLAUDE_ACP_VERSION", BuildConfig.CLAUDE_ACP_VERSION);
            }
            catch (Exception error) {
                runOnUiThread(() -> end(false, "The Android workspace download is unavailable. Check your internet connection and try again later."));
                return;
            }
            runOnUiThread(() -> {
                if (isDestroyed()) return;
                notice = "Installing your workspace. This can take a few minutes…";
                emit();
                TermuxCommand.run(this, "install-termux.sh", environment, (success, output) -> {
                    if (isDestroyed()) return;
                    end(success, success ? "" : "Installation could not finish. Check your internet connection and available storage, then retry.");
                    if (success) check();
                });
            });
        });
    }

    private void scan() {
        // The scanner asks for the camera itself and returns here with the code's text.
        com.google.zxing.integration.android.IntentIntegrator scanner = new com.google.zxing.integration.android.IntentIntegrator(activity);
        scanner.setDesiredBarcodeFormats(com.google.zxing.integration.android.IntentIntegrator.QR_CODE);
        scanner.setPrompt("Scan the code shown on your computer");
        scanner.setBeepEnabled(false);
        scanner.setOrientationLocked(false);
        scanner.setRequestCode(SCAN_REQUEST);
        scanner.initiateScan();
    }

    private void pair(String code) {
        begin("setup_pairing_invite", "Pairing with your computer…");
        RemotePairing.redeem(this, code, (server, problem) -> {
            if (isDestroyed()) return;
            end(server != null, server != null ? "" : problem);
            if (server != null) changed();
        });
    }

    private void join() {
        begin("setup_pairing_join", "Preparing this phone’s code…");
        RemotePairing.join(this, (code, problem) -> {
            if (isDestroyed()) { RemotePairing.stopJoin(); return; }
            joinCode = code;
            end(code != null, code != null ? "" : problem);
        }, (server, problem) -> {
            if (isDestroyed()) return;
            joinCode = null;
            diagnostics.record("setup_pairing_join", "paired", operationSequence, SystemClock.elapsedRealtime() - operationStarted);
            changed();
        });
    }

    private void begin(String operation, String text) {
        busy = true;
        notice = text;
        operationName = operation;
        operationSequence++;
        operationStarted = SystemClock.elapsedRealtime();
        diagnostics.record(operationName, "started", operationSequence, 0);
        emit();
    }

    private void end(boolean success, String text) {
        if (isDestroyed()) return;
        busy = false;
        notice = text;
        diagnostics.record(operationName, success ? "ready" : "failed", operationSequence, SystemClock.elapsedRealtime() - operationStarted);
        emit();
    }

    private void emit() {
        if (isDestroyed()) return;
        try {
            var preferences = getSharedPreferences("connection", MODE_PRIVATE);
            ConnectionStore store = new ConnectionStore(this);
            ConnectionStore.PairedServer computer = store.pairedServer();
            PowerManager power = getSystemService(PowerManager.class);
            JSONObject state = new JSONObject().put("initial", activity.getIntent().getStringExtra("screen"))
                .put("remote", store.usesPaired()).put("paired", computer != null)
                .put("computer", computer == null ? "" : computer.name)
                .put("join", joinCode == null ? JSONObject.NULL : joinCode).put("busy", busy).put("notice", notice)
                .put("termux", installed("com.termux")).put("permission", checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED)
                .put("checks", checks == null ? JSONObject.NULL : checks)
                .put("background", preferences.getBoolean("background", true))
                .put("appBattery", power.isIgnoringBatteryOptimizations(getPackageName()))
                .put("termuxBattery", power.isIgnoringBatteryOptimizations("com.termux"))
                .put("batterySaver", power.isPowerSaveMode())
                .put("notifications", getSystemService(android.app.NotificationManager.class).areNotificationsEnabled());
            view.render(state);
        } catch (Exception ignored) { }
    }

    private boolean installed(String name) {
        try { getPackageManager().getPackageInfo(name, 0); return true; }
        catch (PackageManager.NameNotFoundException error) { return false; }
    }

    private void copy(String command) { getSystemService(ClipboardManager.class).setPrimaryClip(ClipData.newPlainText("OpenAIDE setup", command)); }
    private void launchTermux() {
        Intent launch = getPackageManager().getLaunchIntentForPackage("com.termux");
        if (launch != null) open(launch);
    }
    private void open(Intent intent) {
        try { startActivity(intent); }
        catch (RuntimeException error) { notice = "This option is unavailable on your phone. Open Android Settings to continue."; emit(); }
    }
    private void changed() { view.changed(); }

    void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        if (request == 1) {
            if (results.length > 0 && results[0] == PackageManager.PERMISSION_GRANTED) check();
            else { notice = "Allow Termux command access in OpenAIDE’s Android app permissions to use this phone."; emit(); }
        }
    }

    void onActivityResult(int request, int result, Intent data) {
        if (request != SCAN_REQUEST) return;
        String code = com.google.zxing.integration.android.IntentIntegrator.parseActivityResult(result, data).getContents();
        if (code != null) pair(code);
    }

    void resume() {
        if (recheckOnReturn && !busy) { recheckOnReturn = false; check(); }
        else emit();
    }
    void dispose() {
        if (busy) diagnostics.record(operationName, "view_closed", operationSequence, SystemClock.elapsedRealtime() - operationStarted);
        disposed = true;
        RemotePairing.stopJoin();
        worker.shutdownNow();
    }
    private boolean isDestroyed() { return disposed || activity.isDestroyed(); }
    private void runOnUiThread(Runnable action) { activity.runOnUiThread(action); }
    private void requestPermissions(String[] permissions, int request) { activity.requestPermissions(permissions, request); }
    private void startActivityForResult(Intent intent, int request) { activity.startActivityForResult(intent, request); }
}
