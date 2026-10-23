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
 * a Task works and reads Task state from the App Server this shell holds. For a paired computer it holds no wake
 * lock and does not stop when idle: it keeps a {@link RemoteAttention} watch, so Task
 * notifications still arrive while the WebView is paused. The selected workspace
 * decides the mode on every start, so a restart by the system resumes the same one.
 */
public final class BackgroundService extends Service {
    private static final String TASK_TAG = "task:";
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
    private final AttentionNotifications attention = new AttentionNotifications();
    private long hiddenSince;
    private String notificationText = "";
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    /** Why the service is ending; a stop the service did not ask for is the system's. */
    private String stopReason = "system";
    /** The paired computer's watch, or null while the service serves the workspace on this phone. */
    private RemoteAttention remote;
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
                    handler.post(() -> { if (!destroyed && remote != null) presentAttention(current); });
                }
            });
            remote.start();
        } else if (remote != null) {
            remote.stop();
            remote = null;
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

    private void poll() {
        if (polling || destroyed || remote != null) return;
        handler.removeCallbacks(pollTask);
        polling = true;
        worker.execute(() -> {
            ServerStatus status = null;
            // The App Server this shell holds: in Termux, or on the SSH Computer in use.
            try { status = new ConnectionStore(this).usesSsh() ? SshServer.status(this) : LocalServer.status(this); }
            catch (Exception ignored) { }
            ServerStatus result = status;
            handler.post(() -> {
                polling = false;
                // A switch to the paired computer while this read ran ends the local schedule.
                if (destroyed || remote != null) return;
                long now = SystemClock.elapsedRealtime();
                if (result != null) {
                    policy.observe(result.active, now);
                    presentAttention(result.attention);
                    updateNotification(result.active > 0 ? "Working · safe to lock the screen"
                        : result.waiting > 0 ? "Waiting for your response · battery saving" : "Ready · battery saving");
                } else updateNotification("Connection interrupted · checking again");
                updateLock();
                if (policy.expired(now)) {
                    getSystemService(NotificationManager.class).notify(2, notification("Connection lost. Open OpenAIDE to recover.", "completion"));
                    stop("connection_lost");
                } else if (!visible && result != null && result.active == 0 && now >= settlingUntil) stop("idle");
                else handler.postDelayed(pollTask, result == null ? 10_000 : result.active > 0 || !visible ? 5_000 : 15_000);
            });
        });
    }

    /** Shows each new Task Attention Event that occurred out of sight and withdraws superseded ones. */
    private void presentAttention(List<AttentionNotifications.Attention> current) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        Map<String, String> shown = new HashMap<>();
        for (StatusBarNotification active : manager.getActiveNotifications()) {
            String tag = active.getTag();
            String eventId = active.getNotification().extras.getString("event_id");
            if (tag != null && tag.startsWith(TASK_TAG) && eventId != null) shown.put(tag.substring(TASK_TAG.length()), eventId);
        }
        List<String> stale = AttentionNotifications.stale(shown, current);
        for (String taskId : stale) manager.cancel(TASK_TAG + taskId, TASK_NOTIFICATION);
        AttentionNotifications.Decision decision = attention.observe(current, visible, hiddenSince);
        // Android drops a post the user blocked without telling the app, so the block is recorded instead.
        NotificationChannel channel = manager.getNotificationChannel("completion");
        String blocked = !manager.areNotificationsEnabled() ? "app"
            : channel != null && channel.getImportance() == NotificationManager.IMPORTANCE_NONE ? "channel" : null;
        for (AttentionNotifications.Attention event : decision.show) {
            manager.notify(TASK_TAG + event.taskId, TASK_NOTIFICATION, taskNotification(event));
            diagnostics.record("task_attention_notification", blocked == null ? "shown" : "blocked", 1, 0,
                blocked == null ? Map.of("reason", event.reason, "event_id", event.eventId)
                    : Map.of("reason", event.reason, "event_id", event.eventId, "blocked_by", blocked));
        }
        if (decision.baseline > 0) diagnostics.record("task_attention_notification", "suppressed", decision.baseline, 0, Map.of("cause", "baseline"));
        if (decision.inSight > 0) diagnostics.record("task_attention_notification", "suppressed", decision.inSight, 0, Map.of("cause", "in_sight"));
        if (!stale.isEmpty()) diagnostics.record("task_attention_notification", "withdrawn", stale.size(), 0);
    }

    private void stop(String reason) {
        stopReason = reason;
        stopSelf();
    }

    private Notification taskNotification(AttentionNotifications.Attention event) {
        // A distinct request code per Task keeps each notification's target Task.
        PendingIntent open = PendingIntent.getActivity(this, event.taskId.hashCode(),
            new Intent(this, MainActivity.class).putExtra(MainActivity.EXTRA_TASK_ID, event.taskId),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        android.os.Bundle extras = new android.os.Bundle();
        extras.putString("event_id", event.eventId);
        return new Notification.Builder(this, "completion").setSmallIcon(R.drawable.ic_agent)
            .setContentTitle(AttentionNotifications.title(event.reason)).setContentText(event.title)
            .setContentIntent(open).setAutoCancel(true).setExtras(extras).build();
    }

    private void updateLock() {
        // A paired computer does the work; the phone may sleep while it waits for an answer.
        protection.apply(remote == null && policy.protect(SystemClock.elapsedRealtime(), settlingUntil));
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
        if (protection != null) protection.close();
        stopForeground(STOP_FOREGROUND_REMOVE);
        // Task notifications end with the service, so its stop explains a later missing one.
        diagnostics.record("background_work", "stopped", 0, 0, Map.of("reason", stopReason));
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
