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
    /** The outcome of the last Stop of an SSH Computer; the Remote section shows it. */
    private String sshNotice = "";
    private long operationStarted;
    private int operationSequence;
    private String operationName;
    private JSONObject joinCode;
    private final ComputerSwitch<?> switcher;

    ConnectionController(Activity activity, View view, boolean localTools) {
        super(activity);
        this.activity = activity;
        this.view = view;
        this.localTools = localTools;
        switcher = RemotePairing.computerSwitch(activity, () -> { if (!isDestroyed()) changed(); });
    }

    void receive(JSONObject message) {
        if (disposed) return;
        String command = message.optString("action");
        if (!localTools && !java.util.Arrays.asList("state", "close", "pair_setup", "app_settings", "diagnostics",
            "ssh_add", "use_ssh", "forget_ssh", "stop_ssh", "update_ssh", "background").contains(command)) {
            notice = "Switch to this phone before managing local tools.";
            emit();
            return;
        }
        try { action(message); }
        catch (Exception error) { notice = "That action could not finish. Please try again."; emit(); }
    }

    /**
     * Switches the computer in use, connecting first (ADR-0070); [reply] answers
     * the page once, before a switch that took effect replaces it.
     */
    void switchComputer(JSONObject message, ComputerSwitch.Reply reply) {
        if (disposed) { reply.done(null, "Phone settings are unavailable. Reopen Settings and try again."); return; }
        String action = message.optString("action");
        if (action.equals("cancel_switch")) { reply.done(switcher.cancel() ? "cancelled" : "unchanged", null); return; }
        ConnectionStore store = new ConnectionStore(this);
        ConnectionStore.PairedServer selected = store.usesPaired() ? store.pairedServer() : null;
        String target = action.equals("use_computer") ? message.optString("id") : null;
        String operationId = message.has("operationId") ? message.optString("operationId") : null;
        if (action.equals("forget_computer")) { forget(message.optString("id"), operationId, reply); return; }
        // An SSH Computer in use is a source of its own, so returning to this phone is a switch.
        // TODO(ssh-computers): switch to SSH Computers through this switch too (ADR-0070), so
        // "use_ssh" connects before it replaces the page and the switcher lists them.
        SshComputers.Computer ssh = store.usesSsh() ? store.sshComputer() : null;
        String source = selected != null ? selected.id : ssh != null ? "ssh:" + ssh.id : null;
        switcher.request(source, target, operationId, reply);
    }

    /** Forgets a paired computer; the one in use is left through the switch first, and forgotten only once it took effect. */
    private void forget(String id, String operationId, ComputerSwitch.Reply reply) {
        ComputerForget.Pairings pairings = new ComputerForget.Pairings() {
            @Override public String inUse() {
                ConnectionStore store = new ConnectionStore(ConnectionController.this);
                ConnectionStore.PairedServer selected = store.usesPaired() ? store.pairedServer() : null;
                return selected == null ? null : selected.id;
            }
            @Override public void forget(String forgotten) { new ConnectionStore(ConnectionController.this).forgetPaired(forgotten); }
        };
        ComputerForget.INSTANCE.forget(id, operationId, pairings, switcher,
            (outcome, fields) -> diagnostics.record("connection_computer_forgotten", outcome, 0, 0, fields), this::emit, reply);
    }

    private void action(JSONObject message) throws Exception {
        String action = message.optString("action");
        if (action.equals("state")) { emit(); return; }
        if (busy && !action.equals("close")) return;
        notice = "";
        sshNotice = "";
        switch (action) {
            case "close": view.close(); break;
            case "state": emit(); break;
            case "check": check(); break;
            case "pair_setup": view.setup("remote"); break;
            case "scan": scan(); break;
            case "pair": pair(message.optString("code")); break;
            case "join": join(); break;
            case "join_stop": RemotePairing.stopJoin(); joinCode = null; emit(); break;
            case "ssh_add": SshSetup.start(activity, this::changed); break;
            case "use_ssh":
                if (!new ConnectionStore(this).selectSsh(message.optString("id"))) { emit(); break; }
                RemoteNode.INSTANCE.disconnect();
                SshServer.reset();
                diagnostics.record("connection_ssh_selected", "selected", 0, 0);
                changed();
                break;
            case "forget_ssh": {
                ConnectionStore store = new ConnectionStore(this);
                SshComputers.Computer selected = store.sshComputer();
                boolean current = selected != null && selected.id.equals(message.optString("id"));
                store.forgetSsh(message.optString("id"));
                diagnostics.record("connection_ssh_forgotten", current ? "current" : "other", 0, 0);
                if (current) { SshServer.reset(); changed(); } else emit();
                break;
            }
            case "stop_ssh": stopSsh(message.optString("id")); break;
            case "update_ssh":
                // The Frontend showed what the replacement interrupts; reconnecting replaces it.
                if (!new ConnectionStore(this).usesSsh()) { emit(); break; }
                diagnostics.record("connection_ssh_update", "confirmed", 0, 0);
                SshServer.update();
                changed();
                break;
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
    static String checkEnvironment() { return TermuxCommand.variable("OPENAIDE_VERSION", BundledRuntime.expectedVersion()); }

    private void install() {
        if (checks == null) { check(); return; }
        boolean needsRuntime = !checks.optBoolean("runtime");
        begin("setup_install", "Preparing your workspace download…");
        worker.execute(() -> {
            String environment;
            try {
                environment = (needsRuntime ? (BundledRuntime.available() ? BundledRuntime.installEnvironment(this) : RuntimeRelease.installEnvironment()) : "")
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

    private void stopSsh(String id) {
        ConnectionStore store = new ConnectionStore(this);
        SshComputers.Computer computer = SshComputers.find(store.sshComputers(), id);
        if (computer == null) { emit(); return; }
        begin("ssh_stop", "Stopping OpenAIDE on " + computer.name + "…");
        SshServer.stop(this, computer, store, outcome -> {
            switch (outcome) {
                case "stopped": sshNotice = "OpenAIDE on " + computer.name + " stops once no app is connected to it."; break;
                case "notRunning": sshNotice = "OpenAIDE was not kept running on " + computer.name + "."; break;
                case "stillRunning": sshNotice = "OpenAIDE on " + computer.name + " did not stop yet. Try again in a moment."; break;
                default: sshNotice = SshServer.message(outcome);
            }
            end(java.util.Arrays.asList("stopped", "notRunning", "stillRunning").contains(outcome), "");
            return kotlin.Unit.INSTANCE;
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
            SshComputers.Computer ssh = store.sshComputer();
            PowerManager power = getSystemService(PowerManager.class);
            JSONObject state = new JSONObject().put("initial", activity.getIntent().getStringExtra("screen"))
                .put("connected", activity.getIntent().getBooleanExtra("connected", false))
                .put("remote", store.usesPaired() || ssh != null).put("paired", !store.pairedServers().isEmpty())
                .put("computer", ssh != null ? ssh.name : computer == null ? "" : computer.name)
                .put("computers", computers(store))
                .put("sshComputers", sshComputers(store))
                .put("ssh", sshStatus(ssh))
                .put("sshAvailable", !BuildConfig.SSH_APP_SERVER_SHA256.isEmpty()).put("sshNotice", sshNotice)
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

    /** Every paired computer, with the one remote work goes to marked. */
    private static org.json.JSONArray computers(ConnectionStore store) throws org.json.JSONException {
        ConnectionStore.PairedServer selected = store.usesPaired() ? store.pairedServer() : null;
        org.json.JSONArray computers = new org.json.JSONArray();
        for (ConnectionStore.PairedServer server : store.pairedServers()) {
            computers.put(new JSONObject().put("id", server.id).put("name", server.name)
                .put("current", selected != null && selected.id.equals(server.id)));
        }
        return computers;
    }

    /** Every SSH Computer, with the one in use marked. */
    private static org.json.JSONArray sshComputers(ConnectionStore store) throws org.json.JSONException {
        SshComputers.Computer selected = store.sshComputer();
        org.json.JSONArray computers = new org.json.JSONArray();
        for (SshComputers.Computer computer : store.sshComputers()) {
            computers.put(new JSONObject().put("id", computer.id).put("name", computer.name)
                .put("destination", computer.destination()).put("current", selected != null && selected.id.equals(computer.id)));
        }
        return computers;
    }

    /** What the last launch learned about the App Server on the SSH Computer in use, or null. */
    private static Object sshStatus(SshComputers.Computer selected) throws org.json.JSONException {
        SshServer.Status status = SshServer.status();
        if (selected == null || status == null) return JSONObject.NULL;
        return new JSONObject().put("runningVersion", status.runningVersion)
            .put("updateAvailable", status.updateAvailable).put("suggestLinger", status.suggestLinger);
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

    /** Sends the current state without rechecking this phone's setup. */
    void refresh() { emit(); }

    void resume() {
        if (recheckOnReturn && !busy) { recheckOnReturn = false; check(); }
        else emit();
    }
    void dispose() {
        if (busy) diagnostics.record(operationName, "view_closed", operationSequence, SystemClock.elapsedRealtime() - operationStarted);
        disposed = true;
        // A page that goes away mid-switch cannot be answered; the switch ends with it.
        switcher.cancel();
        RemotePairing.stopJoin();
        worker.shutdownNow();
    }
    private boolean isDestroyed() { return disposed || activity.isDestroyed(); }
    private void runOnUiThread(Runnable action) { activity.runOnUiThread(action); }
    private void requestPermissions(String[] permissions, int request) { activity.requestPermissions(permissions, request); }
    private void startActivityForResult(Intent intent, int request) { activity.startActivityForResult(intent, request); }
}
