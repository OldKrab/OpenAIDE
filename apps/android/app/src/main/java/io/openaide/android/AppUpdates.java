package io.openaide.android;

import android.app.Activity;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.content.ActivityNotFoundException;
import android.content.Context;
import android.content.Intent;
import android.content.SharedPreferences;
import android.content.pm.ApplicationInfo;
import android.content.pm.PackageInfo;
import android.content.pm.PackageInstaller;
import android.content.pm.PackageManager;
import android.net.Uri;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.provider.Settings;
import java.io.ByteArrayOutputStream;
import java.io.File;
import java.io.FileInputStream;
import java.io.FileOutputStream;
import java.io.IOException;
import java.io.InputStream;
import java.io.OutputStream;
import java.net.HttpURLConnection;
import java.net.URL;
import java.nio.charset.StandardCharsets;
import java.security.MessageDigest;
import java.util.Arrays;
import java.util.Set;
import java.util.concurrent.CopyOnWriteArraySet;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;
import java.util.function.Consumer;
import org.json.JSONException;
import org.json.JSONObject;

/**
 * Owns this app's own updates: finding a newer stable release, downloading and verifying
 * its APK, and handing it to Android's package installer. Nothing installs without the
 * user's tap; a scheduled check only posts a notification. The App Server in Termux is
 * not touched: after the new app starts, setup offers its matching runtime as before.
 *
 * State changes on the main thread and is published as the revisioned snapshot the
 * Frontend's update row renders; network and file work runs on one worker thread.
 */
final class AppUpdates {
    static final String CHANNEL = "updates";
    static final String EXTRA_SHOW = "show_updates";
    private static final int NOTIFICATION = 3;
    private static final long CHECK_INTERVAL_MS = 12L * 60 * 60 * 1000;
    // Holds only the application context, so the process-wide instance outlives no Activity.
    @android.annotation.SuppressLint("StaticFieldLeak")
    private static AppUpdates instance;

    private final Context context;
    private final SharedPreferences preferences;
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private final Handler main = new Handler(Looper.getMainLooper());
    private final Set<Consumer<JSONObject>> listeners = new CopyOnWriteArraySet<>();
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private final String unavailableReason;
    private int revision;
    private String kind = "idle";
    private String error;
    private AppRelease.Offer offer;
    private long downloadedBytes;
    private File apk;
    private volatile boolean cancelled;
    /** Install was tapped while Android still withheld "Install unknown apps"; a return resumes it. */
    private boolean awaitingPermission;

    static synchronized AppUpdates get(Context context) {
        if (instance == null) instance = new AppUpdates(context.getApplicationContext());
        return instance;
    }

    private AppUpdates(Context context) {
        this.context = context;
        preferences = context.getSharedPreferences("updates", Context.MODE_PRIVATE);
        // A debug or side-by-side build is signed or named differently from the release it would replace.
        boolean debuggable = (context.getApplicationInfo().flags & ApplicationInfo.FLAG_DEBUGGABLE) != 0;
        unavailableReason = debuggable || !"io.openaide.android".equals(context.getPackageName()) ? "developmentBuild" : null;
        if (unavailableReason != null) kind = "unavailable";
        // An APK left from an earlier run is either installed already or stale; a download starts afresh.
        worker.execute(() -> {
            File[] stale = new File(context.getCacheDir(), "app-update").listFiles();
            if (stale != null) for (File old : stale) old.delete();
        });
    }

    JSONObject snapshot() {
        try {
            JSONObject state = new JSONObject().put("revision", revision).put("installedVersion", BuildConfig.VERSION_NAME).put("kind", kind);
            if (unavailableReason != null) state.put("unavailableReason", unavailableReason);
            if (error != null) state.put("error", error);
            long checked = preferences.getLong("last_checked", 0);
            if (checked > 0) state.put("lastCheckedAtMs", checked);
            if (offer != null) {
                JSONObject published = new JSONObject().put("version", offer.version).put("notes", offer.notes).put("sizeBytes", offer.sizeBytes);
                if (offer.publishedAt != null) published.put("publishedAt", offer.publishedAt);
                state.put("offer", published);
            }
            if ("downloading".equals(kind) && offer != null) {
                state.put("progress", new JSONObject().put("downloadedBytes", downloadedBytes).put("totalBytes", offer.sizeBytes));
            }
            return state;
        } catch (JSONException impossible) {
            throw new IllegalStateException(impossible);
        }
    }

    /** Delivers the current snapshot, then every change, on the main thread. */
    Runnable subscribe(Consumer<JSONObject> listener) {
        listeners.add(listener);
        listener.accept(snapshot());
        return () -> listeners.remove(listener);
    }

    /** A command from the Settings update row; the bridge accepts it only from a shown Settings page. */
    void command(Activity activity, JSONObject command) {
        switch (command.optString("action")) {
            case "update_check": check("user", null); break;
            case "update_download": download(); break;
            case "update_cancel": cancelled = true; break;
            case "update_install": install(activity); break;
            case "update_notes": openNotes(activity, command.optString("version")); break;
            default: break;
        }
    }

    /** Checks when the last check is old enough; the app's start calls this. */
    void checkIfDue() {
        if (System.currentTimeMillis() - preferences.getLong("last_checked", 0) >= CHECK_INTERVAL_MS) check("start", null);
    }

    /**
     * Looks for a newer stable release. A `scheduled` check posts one notification per version;
     * `done` runs later on the main thread once the check settles or is skipped.
     */
    void check(String trigger, Runnable done) {
        if (unavailableReason != null || !Arrays.asList("idle", "available", "failed").contains(kind)) {
            if (done != null) main.post(done);
            return;
        }
        long started = SystemClock.elapsedRealtime();
        log("app_update_check", "outcome=started trigger=" + trigger);
        set("checking", null);
        worker.execute(() -> {
            AppRelease.Offer found = null;
            String failure = null;
            try { found = AppRelease.offer(latestRelease(), BuildConfig.VERSION_NAME); }
            catch (AppRelease.Rejected rejected) { failure = rejected.reason; }
            catch (IOException network) { failure = "network"; }
            final AppRelease.Offer result = found;
            final String reason = failure;
            main.post(() -> {
                long duration = SystemClock.elapsedRealtime() - started;
                if (reason != null) {
                    log("app_update_check", "outcome=failed trigger=" + trigger + " reason=" + reason + " duration_ms=" + duration);
                    diagnostics.record("app_update_check", "failed", 0, duration);
                    // A scheduled check fails quietly; the Settings row shows only what the user asked for.
                    if ("user".equals(trigger)) set("failed", reason.equals("network") ? "network" : "invalidManifest");
                    else set(offer != null ? "available" : "idle", null);
                } else {
                    preferences.edit().putLong("last_checked", System.currentTimeMillis()).apply();
                    offer = result;
                    log("app_update_check", "outcome=" + (result == null ? "current" : "available version=" + result.version)
                        + " trigger=" + trigger + " duration_ms=" + duration);
                    diagnostics.record("app_update_check", result == null ? "current" : "available", 0, duration);
                    set(result == null ? "idle" : "available", null);
                    if (result != null && "scheduled".equals(trigger)) notifyOnce(result.version);
                }
                if (done != null) done.run();
            });
        });
    }

    private void download() {
        if (!"available".equals(kind) && !("failed".equals(kind) && offer != null)) return;
        AppRelease.Offer target = offer;
        long started = SystemClock.elapsedRealtime();
        log("app_update_download", "outcome=started version=" + target.version);
        cancelled = false;
        downloadedBytes = 0;
        set("downloading", null);
        worker.execute(() -> {
            File directory = new File(context.getCacheDir(), "app-update");
            File file = new File(directory, AppRelease.assetName(target.version));
            String failure = null;
            try { fetch(target, directory, file); }
            catch (Cancelled stop) { failure = "cancelled"; }
            catch (AppRelease.Rejected rejected) { failure = rejected.reason; }
            catch (IOException network) { failure = directory.getUsableSpace() < target.sizeBytes ? "insufficient_space" : "network"; }
            if (failure != null) file.delete();
            final String reason = failure;
            main.post(() -> {
                long duration = SystemClock.elapsedRealtime() - started;
                log("app_update_download", "outcome=" + (reason == null ? "ready" : reason.equals("cancelled") ? "cancelled" : "failed reason=" + reason)
                    + " version=" + target.version + " bytes=" + downloadedBytes + " duration_ms=" + duration);
                diagnostics.record("app_update_download", reason == null ? "ready" : reason, 0, duration);
                if (reason == null) { apk = file; set("readyToUpdate", null); }
                else if (reason.equals("cancelled")) set("available", null);
                else set("failed", downloadError(reason));
            });
        });
    }

    private void install(Activity activity) {
        if (!"readyToUpdate".equals(kind) || apk == null) return;
        if (!context.getPackageManager().canRequestPackageInstalls()) {
            // Android grants "Install unknown apps" per app, once; the user returns with Back.
            awaitingPermission = true;
            log("app_update_install", "outcome=permission_requested version=" + offer.version);
            diagnostics.record("app_update_install", "permission_requested", 0, 0);
            try {
                activity.startActivity(new Intent(Settings.ACTION_MANAGE_UNKNOWN_APP_SOURCES, Uri.parse("package:" + context.getPackageName())));
            } catch (ActivityNotFoundException missing) {
                awaitingPermission = false;
                set("failed", "installFailed");
            }
            return;
        }
        awaitingPermission = false;
        File file = apk;
        String version = offer.version;
        long started = SystemClock.elapsedRealtime();
        log("app_update_install", "outcome=started version=" + version);
        set("applying", null);
        worker.execute(() -> {
            String failure = null;
            try { commit(file); }
            catch (IOException | RuntimeException error) { failure = error.getClass().getSimpleName(); }
            final String reason = failure;
            main.post(() -> {
                long duration = SystemClock.elapsedRealtime() - started;
                if (reason == null) { log("app_update_install", "outcome=committed version=" + version + " duration_ms=" + duration); return; }
                log("app_update_install", "outcome=failed reason=session_" + reason + " version=" + version + " duration_ms=" + duration);
                diagnostics.record("app_update_install", "failed", 0, duration);
                set("failed", "installFailed");
            });
        });
    }

    /** The activity is shown again, possibly back from the "Install unknown apps" screen. */
    void resumed(Activity activity) {
        if (!awaitingPermission) return;
        awaitingPermission = false;
        boolean granted = context.getPackageManager().canRequestPackageInstalls();
        log("app_update_install", "outcome=" + (granted ? "permission_granted" : "permission_denied"));
        if (granted) install(activity);
    }

    /** The package installer's report for the session this app committed; see AppUpdateReceiver. */
    void installStatus(Intent intent) {
        int status = intent.getIntExtra(PackageInstaller.EXTRA_STATUS, PackageInstaller.STATUS_FAILURE);
        if (status == PackageInstaller.STATUS_PENDING_USER_ACTION) {
            Intent confirm = intent.getParcelableExtra(Intent.EXTRA_INTENT);
            log("app_update_install", "outcome=confirmation_requested");
            if (confirm != null) {
                try { context.startActivity(confirm.addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)); return; }
                catch (ActivityNotFoundException ignored) { }
            }
            set("failed", "installFailed");
            return;
        }
        // Success replaces this process, so this log line is the last one from the old version.
        log("app_update_install", "outcome=" + (status == PackageInstaller.STATUS_SUCCESS ? "installed" : "failed reason=status_" + status));
        diagnostics.record("app_update_install", status == PackageInstaller.STATUS_SUCCESS ? "installed" : "failed", status, 0);
        if (status == PackageInstaller.STATUS_SUCCESS) return;
        if (status == PackageInstaller.STATUS_FAILURE_ABORTED) set("readyToUpdate", null);
        else if (status == PackageInstaller.STATUS_FAILURE_CONFLICT || status == PackageInstaller.STATUS_FAILURE_INCOMPATIBLE) set("failed", "untrustedArtifact");
        else if (status == PackageInstaller.STATUS_FAILURE_STORAGE) set("failed", "insufficientSpace");
        else set("failed", "installFailed");
    }

    private void openNotes(Activity activity, String version) {
        try { activity.startActivity(new Intent(Intent.ACTION_VIEW, Uri.parse(AppRelease.notesUrl(version)))); }
        catch (IllegalArgumentException | ActivityNotFoundException ignored) { }
    }

    private void set(String next, String failure) {
        kind = next;
        error = failure;
        publish();
    }

    private void publish() {
        revision++;
        JSONObject state = snapshot();
        for (Consumer<JSONObject> listener : listeners) listener.accept(state);
    }

    private void notifyOnce(String version) {
        if (version.equals(preferences.getString("notified_version", null))) return;
        NotificationManager manager = context.getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel(CHANNEL, "App updates", NotificationManager.IMPORTANCE_DEFAULT));
        if (!manager.areNotificationsEnabled()) {
            log("app_update_notification", "outcome=disabled version=" + version);
            return;
        }
        PendingIntent open = PendingIntent.getActivity(context, NOTIFICATION,
            new Intent(context, MainActivity.class).putExtra(EXTRA_SHOW, true).addFlags(Intent.FLAG_ACTIVITY_SINGLE_TOP),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(context, CHANNEL).setSmallIcon(R.drawable.ic_agent)
            .setContentTitle("OpenAIDE " + version + " is available")
            .setContentText("Tap to see what’s new and update.")
            .setContentIntent(open).setAutoCancel(true).build();
        manager.notify(NOTIFICATION, notification);
        preferences.edit().putString("notified_version", version).apply();
        log("app_update_notification", "outcome=posted version=" + version);
    }

    private static String latestRelease() throws IOException, AppRelease.Rejected {
        HttpURLConnection request = (HttpURLConnection) new URL(AppRelease.LATEST).openConnection();
        request.setConnectTimeout(10_000);
        request.setReadTimeout(15_000);
        request.setInstanceFollowRedirects(false);
        request.setRequestProperty("Accept", "application/vnd.github+json");
        try {
            if (request.getResponseCode() != 200) throw new AppRelease.Rejected("release_unavailable");
            try (InputStream input = request.getInputStream(); ByteArrayOutputStream output = new ByteArrayOutputStream()) {
                byte[] buffer = new byte[8192];
                int count;
                while ((count = input.read(buffer)) != -1) {
                    output.write(buffer, 0, count);
                    if (output.size() > AppRelease.MAX_RESPONSE_BYTES) throw new AppRelease.Rejected("release_too_large");
                }
                return new String(output.toByteArray(), StandardCharsets.UTF_8);
            }
        } finally {
            request.disconnect();
        }
    }

    private static final class Cancelled extends Exception { }

    /** Streams the APK to `file`, then proves its digest, package, version and signer before it is offered. */
    private void fetch(AppRelease.Offer target, File directory, File file) throws IOException, AppRelease.Rejected, Cancelled {
        File[] stale = directory.listFiles();
        if (stale != null) for (File old : stale) old.delete();
        if (!directory.isDirectory() && !directory.mkdirs()) throw new IOException("Update directory unavailable");
        // GitHub redirects the release address to its storage host; HttpURLConnection follows https-to-https only.
        HttpURLConnection request = (HttpURLConnection) new URL(target.url).openConnection();
        request.setConnectTimeout(10_000);
        request.setReadTimeout(30_000);
        try {
            if (request.getResponseCode() != 200) throw new IOException("Unexpected download status");
            MessageDigest digest = MessageDigest.getInstance("SHA-256");
            long lastPublished = 0;
            try (InputStream input = request.getInputStream(); OutputStream output = new FileOutputStream(file)) {
                byte[] buffer = new byte[65536];
                int count;
                long total = 0;
                while ((count = input.read(buffer)) != -1) {
                    if (cancelled) throw new Cancelled();
                    total += count;
                    if (total > target.sizeBytes) throw new AppRelease.Rejected("asset_size_mismatch");
                    output.write(buffer, 0, count);
                    digest.update(buffer, 0, count);
                    final long progress = total;
                    // About one progress snapshot per megabyte keeps the Settings row live without flooding the bridge.
                    if (progress - lastPublished >= 1 << 20 || progress == target.sizeBytes) {
                        lastPublished = progress;
                        main.post(() -> { if ("downloading".equals(kind)) { downloadedBytes = progress; publish(); } });
                    }
                }
                if (total != target.sizeBytes) throw new AppRelease.Rejected("asset_size_mismatch");
            }
            if (!target.sha256.equals(hex(digest.digest()))) throw new AppRelease.Rejected("digest_mismatch");
        } catch (java.security.NoSuchAlgorithmException impossible) {
            throw new IllegalStateException(impossible);
        } finally {
            request.disconnect();
        }
        verifyPackage(target, file);
    }

    /** Android refuses a differently signed update anyway; checking first names the failure. */
    private void verifyPackage(AppRelease.Offer target, File file) throws AppRelease.Rejected {
        PackageManager packages = context.getPackageManager();
        PackageInfo archive = packages.getPackageArchiveInfo(file.getPath(), Build.VERSION.SDK_INT >= 28 ? PackageManager.GET_SIGNING_CERTIFICATES : 0);
        if (archive == null || !context.getPackageName().equals(archive.packageName) || !target.version.equals(archive.versionName)) {
            throw new AppRelease.Rejected("package_mismatch");
        }
        // Before Android 9 only the installer's own signature check applies.
        if (Build.VERSION.SDK_INT < 28) return;
        try {
            PackageInfo installed = packages.getPackageInfo(context.getPackageName(), PackageManager.GET_SIGNING_CERTIFICATES);
            if (archive.signingInfo == null || installed.signingInfo == null
                || !Arrays.equals(archive.signingInfo.getApkContentsSigners(), installed.signingInfo.getApkContentsSigners())) {
                throw new AppRelease.Rejected("signer_mismatch");
            }
        } catch (PackageManager.NameNotFoundException impossible) {
            throw new AppRelease.Rejected("package_mismatch");
        }
    }

    private void commit(File file) throws IOException {
        PackageInstaller installer = context.getPackageManager().getPackageInstaller();
        PackageInstaller.SessionParams params = new PackageInstaller.SessionParams(PackageInstaller.SessionParams.MODE_FULL_INSTALL);
        params.setAppPackageName(context.getPackageName());
        params.setSize(file.length());
        // The user already tapped Install; Android may still confirm, notably the first time.
        if (Build.VERSION.SDK_INT >= 31) params.setRequireUserAction(PackageInstaller.SessionParams.USER_ACTION_NOT_REQUIRED);
        int id = installer.createSession(params);
        try (PackageInstaller.Session session = installer.openSession(id)) {
            try (InputStream input = new FileInputStream(file); OutputStream output = session.openWrite("openaide.apk", 0, file.length())) {
                byte[] buffer = new byte[65536];
                int count;
                while ((count = input.read(buffer)) != -1) output.write(buffer, 0, count);
                session.fsync(output);
            }
            // The installer fills in status extras, so this PendingIntent must stay mutable.
            int flags = PendingIntent.FLAG_UPDATE_CURRENT | (Build.VERSION.SDK_INT >= 31 ? PendingIntent.FLAG_MUTABLE : 0);
            PendingIntent status = PendingIntent.getBroadcast(context, id,
                new Intent(context, AppUpdateReceiver.class).setPackage(context.getPackageName()), flags);
            session.commit(status.getIntentSender());
        } catch (IOException | RuntimeException error) {
            installer.abandonSession(id);
            throw error;
        }
    }

    private static String downloadError(String reason) {
        switch (reason) {
            case "insufficient_space": return "insufficientSpace";
            case "digest_mismatch": case "package_mismatch": case "signer_mismatch": case "asset_size_mismatch": return "untrustedArtifact";
            default: return "downloadFailed";
        }
    }

    private static String hex(byte[] bytes) {
        StringBuilder text = new StringBuilder(bytes.length * 2);
        for (byte value : bytes) text.append(String.format("%02x", value));
        return text.toString();
    }

    private static void log(String operation, String fields) {
        android.util.Log.i("OpenAIDE", operation + " " + fields);
    }
}
