package io.openaide.android;

import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.os.Handler;
import android.os.IBinder;
import android.os.Looper;
import android.os.PowerManager;
import android.os.SystemClock;
import android.service.notification.StatusBarNotification;
import java.util.HashMap;
import java.util.List;
import java.util.Locale;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

/**
 * The ongoing notification that keeps OpenAIDE working while it is off the screen.
 *
 * For the workspace on this phone or on an SSH Computer it keeps the phone awake while
 * a Task works and reads Task state from the App Server this shell holds. For a paired
 * computer it does not stop when idle: it keeps a {@link RemoteAttention} watch, so Task
 * notifications still arrive while the WebView is paused, and it also watches this phone's
 * App Server if that is already running, so the phone's own Tasks keep it awake and notify
 * too ({@link BackgroundSources}). The selected workspace decides the mode on every start,
 * so a restart by the system resumes the same one.
 */
public final class BackgroundService extends Service {
    private static final int TASK_NOTIFICATION = 3;
    private WorkProtection protection;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private final PowerPolicy policy = new PowerPolicy(SystemClock.elapsedRealtime());
    private boolean visible;
    private boolean destroyed;
    private boolean polling;
    private final Runnable pollTask = this::poll;
    private long settlingUntil;
    /** Each source has its own baseline: the first list read from it stays silent. */
    private final AttentionNotifications localAttention = new AttentionNotifications();
    private final AttentionNotifications pairedAttention = new AttentionNotifications();
    private long hiddenSince;
    private String notificationText = "";
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    /** Why the service is ending; a stop the service did not ask for is the system's. */
    private String stopReason = "system";
    /** The paired computer's watch, or null while the service serves the workspace on this phone. */
    private RemoteAttention remote;
    /** Whether this phone's own App Server is watched while the paired computer is in use. */
    private boolean watchingLocal;
    /** Failed reads in a row of the phone's App Server while paired; only the transitions are logged. */
    private int localFailures;
    private boolean modeChosen;
    /** An outdated computer is reported once per process, not on every return to the app. */
    private static boolean outdatedReported;

    @Override public void onCreate() {
        super.onCreate();
        NotificationManager manager = getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel("background", "Background work", NotificationManager.IMPORTANCE_LOW));
        manager.createNotificationChannel(new NotificationChannel("completion", "Work updates", NotificationManager.IMPORTANCE_DEFAULT));
        protection = new WorkProtection(getSystemService(PowerManager.class), "OpenAIDE:active-work");
        Boolean paired = pairedWorkspace();
        startForeground(1, notification(Boolean.TRUE.equals(paired) ? "Connecting to your computer…"
            : Boolean.FALSE.equals(paired) && sshWorkspace() ? "Checking work on your SSH computer…" : "Checking local work…", "background"));
        diagnostics.record("background_work", "started", 0, 0);
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && "stop".equals(intent.getAction())) {
            getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("background", false).apply();
            stop("turned_off");
            return START_NOT_STICKY;
        }
        if (!getSharedPreferences("connection", MODE_PRIVATE).getBoolean("background", true)) {
            stop("disabled");
            return START_NOT_STICKY;
        }
        if (intent != null && intent.hasExtra("visible")) {
            boolean next = intent.getBooleanExtra("visible", false);
            if (!next && (visible || hiddenSince == 0)) hiddenSince = System.currentTimeMillis();
            visible = next;
            if (!visible) settlingUntil = SystemClock.elapsedRealtime() + 10_000;
        }
        if (!chooseMode()) return START_NOT_STICKY;
        updateLock();
        poll();
        return START_STICKY;
    }

    /** Whether the selected workspace is the paired computer; null when the choice cannot be read. */
    private Boolean pairedWorkspace() {
        try { return new ConnectionStore(this).usesPaired(); }
        catch (RuntimeException unavailable) { return null; }
    }

    /** Whether the selected workspace is an SSH Computer; false when the choice cannot be read. */
    private boolean sshWorkspace() {
        try { return new ConnectionStore(this).usesSsh(); }
        catch (RuntimeException unavailable) { return false; }
    }

    /** Follows the selected workspace; false when the service stopped instead. */
    private boolean chooseMode() {
        Boolean paired = pairedWorkspace();
        if (paired == null) { stop("storage_unavailable"); return false; }
        if (modeChosen && paired == (remote != null)) return true;
        boolean switched = modeChosen;
        modeChosen = true;
        if (paired) {
            handler.removeCallbacks(pollTask);
            remote = new RemoteAttention(this, new RemoteAttention.Events() {
                @Override public void state(RemoteAttention.State state) { handler.post(() -> remoteState(state)); }
                @Override public void attention(List<AttentionNotifications.Attention> current) {
                    handler.post(() -> { if (!destroyed && remote != null) presentAttention(BackgroundSources.Source.PAIRED, current); });
                }
            });
            remote.start();
            // The first read tells whether this phone's App Server runs; the watch never starts it.
            watchingLocal = true;
            localFailures = 0;
            diagnostics.record("local_watch", "started", 0, 0, Map.of("source", BackgroundSources.Source.THIS_DEVICE.wire));
        } else if (remote != null) {
            remote.stop();
            remote = null;
            endLocalWatch("mode_changed");
            // The phone's reads start over, so a long paired stretch is not taken for a lost server.
            policy.observe(0, SystemClock.elapsedRealtime());
        }
        diagnostics.record("background_work", paired ? "paired" : sshWorkspace() ? "ssh" : "local", 0, 0, Map.of("switched", switched));
        return true;
    }

    private void remoteState(RemoteAttention.State state) {
        if (destroyed || remote == null) return;
        switch (state) {
            case CONNECTING: updateNotification("Connecting to your computer…"); return;
            case WATCHING: updateNotification("Connected · notifies you when a task needs you"); return;
            case RECONNECTING: updateNotification("Reconnecting to your computer…"); return;
            default: break;
        }
        // The workspace says the same while it is on the screen; a notification is for when it is not.
        String notice = state == RemoteAttention.State.NOT_TRUSTED ? "This phone was removed from your computer. Open OpenAIDE to pair it again."
            : state == RemoteAttention.State.INCOMPATIBLE ? "OpenAIDE on this phone and on your computer are different versions. Update both to get task notifications."
            : state == RemoteAttention.State.OUTDATED ? "Update OpenAIDE on your computer to get task notifications on this phone."
            : "Task notifications stopped. Open OpenAIDE to turn them back on.";
        boolean outdated = state == RemoteAttention.State.OUTDATED;
        if (outdated ? !outdatedReported : !visible) getSystemService(NotificationManager.class).notify(2, notification(notice, "completion"));
        if (outdated) outdatedReported = true;
        stop("remote_" + state.name().toLowerCase(Locale.ROOT));
    }

    /** Reads this phone's App Server: the workspace in use, or alongside the paired computer while it is running. */
    private void poll() {
        boolean paired = remote != null;
        if (polling || destroyed || (paired && !watchingLocal)) return;
        handler.removeCallbacks(pollTask);
        polling = true;
        worker.execute(() -> {
            ServerStatus status = null;
            boolean running = true;
            // The phone's App Server is started only while it is the workspace in use; while an
            // SSH Computer is in use, the App Server this shell holds is the one there.
            try {
                status = paired ? LocalServer.statusIfRunning()
                    : new ConnectionStore(this).usesSsh() ? SshServer.status(this) : LocalServer.status(this);
                running = status != null;
            }
            catch (Exception ignored) { }
            ServerStatus result = status;
            boolean serverRunning = running;
            handler.post(() -> {
                polling = false;
                if (destroyed) return;
                // A switch of the workspace while this read ran starts the schedule of the new mode.
                if ((remote != null) != paired) { poll(); return; }
                long now = SystemClock.elapsedRealtime();
                if (result != null) {
                    policy.observe(result.active, now);
                    presentAttention(BackgroundSources.Source.THIS_DEVICE, result.attention);
                    // While paired, the ongoing notification tells about the paired computer.
                    if (!paired) updateNotification(result.active > 0 ? "Working · safe to lock the screen"
                        : result.waiting > 0 ? "Waiting for your response · battery saving" : "Ready · battery saving");
                } else if (!paired) updateNotification("Connection interrupted · checking again");
                if (paired) recordLocalRead(result != null, serverRunning);
                BackgroundSources.Next next = BackgroundSources.afterLocalRead(paired, serverRunning, result, policy.expired(now), visible, now, settlingUntil);
                if (next == BackgroundSources.Next.END_WATCH) {
                    endLocalWatch("not_running");
                    policy.observe(0, now);
                }
                updateLock();
                switch (next) {
                    case STOP_LOST:
                        getSystemService(NotificationManager.class).notify(2, notification("Connection lost. Open OpenAIDE to recover.", "completion"));
                        stop("connection_lost");
                        break;
                    case STOP_IDLE: stop("idle"); break;
                    case END_WATCH: break;
                    default: handler.postDelayed(pollTask, BackgroundSources.delay(paired, result, visible));
                }
            });
        });
    }

    /** Logs the phone's reads while paired by transition only: a healthy schedule stays quiet. */
    private void recordLocalRead(boolean read, boolean running) {
        if (!running) return;
        if (!read && ++localFailures == 1) diagnostics.record("local_watch", "retrying", localFailures, 0, Map.of("source", BackgroundSources.Source.THIS_DEVICE.wire));
        if (read && localFailures > 0) {
            diagnostics.record("local_watch", "recovered", localFailures, 0, Map.of("source", BackgroundSources.Source.THIS_DEVICE.wire));
            localFailures = 0;
        }
    }

    private void endLocalWatch(String reason) {
        if (!watchingLocal) return;
        watchingLocal = false;
        diagnostics.record("local_watch", "ended", localFailures, 0, Map.of("source", BackgroundSources.Source.THIS_DEVICE.wire, "reason", reason));
    }

    /** Shows each new Task Attention Event of [source] that occurred out of sight and withdraws its superseded ones. */
    private void presentAttention(BackgroundSources.Source source, List<AttentionNotifications.Attention> current) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        Map<String, String> shown = new HashMap<>();
        for (StatusBarNotification active : manager.getActiveNotifications()) {
            String tag = active.getTag();
            String eventId = active.getNotification().extras.getString("event_id");
            if (tag != null && tag.startsWith(source.tag) && eventId != null) shown.put(tag.substring(source.tag.length()), eventId);
        }
        List<String> stale = AttentionNotifications.stale(shown, current);
        for (String taskId : stale) manager.cancel(source.tag + taskId, TASK_NOTIFICATION);
        // The workspace not in use is never on the screen, so none of its events occurred in sight.
        boolean inUse = BackgroundSources.inUse(source, remote != null);
        AttentionNotifications attention = source == BackgroundSources.Source.PAIRED ? pairedAttention : localAttention;
        AttentionNotifications.Decision decision = attention.observe(current, inUse && visible, inUse ? hiddenSince : 0);
        // Android drops a post the user blocked without telling the app, so the block is recorded instead.
        NotificationChannel channel = manager.getNotificationChannel("completion");
        String blocked = !manager.areNotificationsEnabled() ? "app"
            : channel != null && channel.getImportance() == NotificationManager.IMPORTANCE_NONE ? "channel" : null;
        for (AttentionNotifications.Attention event : decision.show) {
            manager.notify(source.tag + event.taskId, TASK_NOTIFICATION, taskNotification(source, event));
            diagnostics.record("task_attention_notification", blocked == null ? "shown" : "blocked", 1, 0,
                blocked == null ? Map.of("reason", event.reason, "event_id", event.eventId, "source", source.wire)
                    : Map.of("reason", event.reason, "event_id", event.eventId, "blocked_by", blocked, "source", source.wire));
        }
        if (decision.baseline > 0) diagnostics.record("task_attention_notification", "suppressed", decision.baseline, 0, Map.of("cause", "baseline", "source", source.wire));
        if (decision.inSight > 0) diagnostics.record("task_attention_notification", "suppressed", decision.inSight, 0, Map.of("cause", "in_sight", "source", source.wire));
        if (!stale.isEmpty()) diagnostics.record("task_attention_notification", "withdrawn", stale.size(), 0, Map.of("source", source.wire));
    }

    private void stop(String reason) {
        stopReason = reason;
        stopSelf();
    }

    private Notification taskNotification(BackgroundSources.Source source, AttentionNotifications.Attention event) {
        // The source tells the tap whether the phone must become the workspace in use first.
        PendingIntent open = PendingIntent.getActivity(this, source.requestCode(event.taskId),
            new Intent(this, MainActivity.class).putExtra(MainActivity.EXTRA_TASK_ID, event.taskId)
                .putExtra(BackgroundSources.EXTRA_TASK_SOURCE, source.wire),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        android.os.Bundle extras = new android.os.Bundle();
        extras.putString("event_id", event.eventId);
        return new Notification.Builder(this, "completion").setSmallIcon(R.drawable.ic_agent)
            .setContentTitle(AttentionNotifications.title(event.reason)).setContentText(event.title)
            .setContentIntent(open).setAutoCancel(true).setExtras(extras).build();
    }

    private void updateLock() {
        // A paired computer does its own work; only this phone's working Tasks keep it awake.
        protection.apply(BackgroundSources.protect(remote != null, watchingLocal, policy, SystemClock.elapsedRealtime(), settlingUntil));
    }

    private void updateNotification(String text) {
        if (notificationText.equals(text)) return;
        notificationText = text;
        getSystemService(NotificationManager.class).notify(1, notification(text, "background"));
    }

    private Notification notification(String text, String channel) {
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent settings = PendingIntent.getActivity(this, 1, new Intent(this, MainActivity.class).putExtra("show_settings", true), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 2, new Intent(this, BackgroundService.class).setAction("stop"), PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification.Builder builder = new Notification.Builder(this, channel).setSmallIcon(R.drawable.ic_agent)
            .setContentTitle("OpenAIDE").setContentText(text).setContentIntent(open).setOnlyAlertOnce(true);
        if ("background".equals(channel)) builder.setOngoing(true)
            .addAction(new Notification.Action.Builder(null, "Settings", settings).build())
            .addAction(new Notification.Action.Builder(null, "Turn off", stop).build());
        else builder.setAutoCancel(true);
        return builder.build();
    }

    @Override public void onDestroy() {
        destroyed = true;
        handler.removeCallbacksAndMessages(null);
        worker.shutdownNow();
        if (remote != null) remote.stop();
        endLocalWatch(stopReason);
        if (protection != null) protection.close();
        stopForeground(STOP_FOREGROUND_REMOVE);
        // Task notifications end with the service, so its stop explains a later missing one.
        diagnostics.record("background_work", "stopped", 0, 0, Map.of("reason", stopReason));
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
