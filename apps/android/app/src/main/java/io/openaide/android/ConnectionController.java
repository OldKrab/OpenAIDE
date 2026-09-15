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
        void scanned(String address);
    }
    private static final String PERMISSION = "com.termux.permission.RUN_COMMAND";
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

    ConnectionController(Activity activity, View view, boolean localTools) {
        super(activity);
        this.activity = activity;
        this.view = view;
        this.localTools = localTools;
    }

    void receive(JSONObject message) {
        if (disposed) return;
        String command = message.optString("action");
        if (!localTools && !java.util.Arrays.asList("state", "close", "remote", "local", "qr", "app_settings", "diagnostics").contains(command)) {
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
            case "remote": saveRemote(message); break;
            case "install": install(); break;
            case "termux": launchTermux(); break;
            case "get_termux": open(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/termux/termux-app/releases"))); break;
            case "access":
                copy("mkdir -p ~/.termux; printf '\\nallow-external-apps=true\\n' >> ~/.termux/termux.properties; termux-reload-settings");
                recheckOnReturn = true;
                launchTermux();
                break;
            case "grant": requestPermissions(new String[]{PERMISSION}, 1); break;
            case "signin": copy("codex login"); recheckOnReturn = true; launchTermux(); break;
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
            case "repair": repair(); break;
            case "boot": boot(); break;
            case "get_boot": open(new Intent(Intent.ACTION_VIEW, Uri.parse("https://github.com/termux/termux-boot#installation"))); break;
            case "qr": startActivityForResult(new Intent(Intent.ACTION_OPEN_DOCUMENT).addCategory(Intent.CATEGORY_OPENABLE).setType("image/*"), 4); break;
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
        TermuxCommand.run(this, "check-termux.sh", "", false, (success, output) -> {
            if (isDestroyed()) return;
            checks = null;
            if (success) try { checks = new JSONObject(output.trim()); } catch (Exception ignored) { success = false; }
            end(success, success ? "" : "Termux needs permission to connect. Complete the one-time connection step below.");
        });
    }

    private void install() {
        if (checks == null) { check(); return; }
        boolean needsRuntime = !checks.optBoolean("runtime") || !checks.optBoolean("frontend");
        begin("setup_install", "Preparing your workspace download…");
        worker.execute(() -> {
            String environment;
            try { environment = needsRuntime ? RuntimeRelease.installEnvironment() : ""; }
            catch (Exception error) {
                runOnUiThread(() -> end(false, "The Android workspace download is unavailable. Check your internet connection and try again later."));
                return;
            }
            runOnUiThread(() -> {
                if (isDestroyed()) return;
                notice = "Installing your workspace. This can take a few minutes…";
                emit();
                TermuxCommand.run(this, "install-termux.sh", environment, false, (success, output) -> {
                    if (isDestroyed()) return;
                    end(success, success ? "" : "Installation could not finish. Check your internet connection and available storage, then retry.");
                    if (success) check();
                });
            });
        });
    }

    private void saveRemote(JSONObject message) throws Exception {
        ConnectionProfile candidate;
        try { candidate = new ConnectionProfile(message.optString("address"), message.optString("username"), message.optString("password"), false); }
        catch (RuntimeException error) { notice = "Enter an HTTPS server address, username and password. Use the address only, without a page path."; emit(); return; }
        begin("setup_remote_connect", "Connecting securely…");
        worker.execute(() -> {
            boolean connected;
            try { ServerStatus.read(candidate); connected = true; }
            catch (Exception error) { connected = false; }
            boolean success = connected;
            runOnUiThread(() -> {
                if (isDestroyed()) return;
                if (!success) { end(false, "Could not connect. Check the address and sign-in details, and make sure your computer or VPN is online."); return; }
                try { new ConnectionStore(this).saveRemote(candidate); end(true, ""); changed(); }
                catch (RuntimeException error) { end(false, "Secure storage is unavailable. Restart OpenAIDE and try again."); }
            });
        });
    }

    private void repair() {
        begin("setup_pairing", "Reconnecting to Termux…");
        TermuxCommand.run(this, "pair-termux.sh", "", false, (success, output) -> {
            if (isDestroyed()) return;
            boolean repaired = success && output.trim().matches("[a-f0-9]{64}");
            if (repaired) {
                getSharedPreferences("connection", MODE_PRIVATE).edit().putString("password", output.trim()).apply();
                new ConnectionStore(this).selectLocal();
            }
            end(repaired, repaired ? "" : "No saved connection was found. Set up this phone again; your projects and history will not be deleted.");
            if (repaired) changed();
        });
    }

    private void boot() {
        if (!installed("com.termux.boot")) { notice = "Install Termux:Boot from the same source as Termux, then return here."; emit(); return; }
        begin("setup_boot", "Setting up startup after reboot…");
        TermuxCommand.run(this, "configure-boot.sh", "", false, (success, output) -> {
            if (isDestroyed()) return;
            end(success, success ? "Startup is configured. Open Termux:Boot once to enable it." : "Open your local workspace first, then try again.");
            if (success) {
                Intent launch = getPackageManager().getLaunchIntentForPackage("com.termux.boot");
                if (launch != null) open(launch);
            }
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
            PowerManager power = getSystemService(PowerManager.class);
            JSONObject state = new JSONObject().put("initial", activity.getIntent().getStringExtra("screen"))
                .put("remote", preferences.getBoolean("remote", false)).put("address", preferences.getString("remote_url", ""))
                .put("username", preferences.getString("remote_user", "")).put("busy", busy).put("notice", notice)
                .put("termux", installed("com.termux")).put("permission", checkSelfPermission(PERMISSION) == PackageManager.PERMISSION_GRANTED)
                .put("checks", checks == null ? JSONObject.NULL : checks)
                .put("background", preferences.getBoolean("background", true))
                .put("appBattery", power.isIgnoringBatteryOptimizations(getPackageName()))
                .put("termuxBattery", power.isIgnoringBatteryOptimizations("com.termux"))
                .put("batterySaver", power.isPowerSaveMode()).put("boot", installed("com.termux.boot"))
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
        if (request != 4 || result != Activity.RESULT_OK || data == null || data.getData() == null) return;
        begin("setup_qr", "Reading connection image…");
        worker.execute(() -> {
            String address;
            try { address = QrConnection.read(this, data.getData()); } catch (Exception error) { address = null; }
            String scanned = address;
            runOnUiThread(() -> {
                end(scanned != null, scanned == null ? "Choose a QR image containing an HTTPS server address." : "");
                if (scanned != null && !isDestroyed()) view.scanned(scanned);
            });
        });
    }


    void resume() {
        if (recheckOnReturn && !busy) { recheckOnReturn = false; check(); }
        else emit();
    }
    void dispose() {
        if (busy) diagnostics.record(operationName, "view_closed", operationSequence, SystemClock.elapsedRealtime() - operationStarted);
        disposed = true;
        worker.shutdownNow();
    }
    private boolean isDestroyed() { return disposed || activity.isDestroyed(); }
    private void runOnUiThread(Runnable action) { activity.runOnUiThread(action); }
    private void requestPermissions(String[] permissions, int request) { activity.requestPermissions(permissions, request); }
    private void startActivityForResult(Intent intent, int request) { activity.startActivityForResult(intent, request); }
}
