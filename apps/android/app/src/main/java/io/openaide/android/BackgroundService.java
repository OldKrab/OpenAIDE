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
import android.util.Log;
import java.util.HashMap;
import java.util.Map;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

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

    @Override public void onCreate() {
        super.onCreate();
        NotificationManager manager = getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel("background", "Background work", NotificationManager.IMPORTANCE_LOW));
        manager.createNotificationChannel(new NotificationChannel("completion", "Work updates", NotificationManager.IMPORTANCE_DEFAULT));
        protection = new WorkProtection(getSystemService(PowerManager.class), "OpenAIDE:active-work");
        startForeground(1, notification("Checking local work…", "background"));
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && "stop".equals(intent.getAction())) {
            getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("background", false).apply();
            stopSelf();
            return START_NOT_STICKY;
        }
        if (!getSharedPreferences("connection", MODE_PRIVATE).getBoolean("background", true)) {
            stopSelf();
            return START_NOT_STICKY;
        }
        if (intent != null && intent.hasExtra("visible")) {
            boolean next = intent.getBooleanExtra("visible", false);
            if (!next && (visible || hiddenSince == 0)) hiddenSince = System.currentTimeMillis();
            visible = next;
            if (!visible) settlingUntil = SystemClock.elapsedRealtime() + 10_000;
        }
        updateLock();
        poll();
        return START_STICKY;
    }

    private void poll() {
        if (polling || destroyed) return;
        handler.removeCallbacks(pollTask);
        polling = true;
        worker.execute(() -> {
            ServerStatus status = null;
            try { status = LocalServer.status(this); }
            catch (Exception ignored) { }
            ServerStatus result = status;
            handler.post(() -> {
                polling = false;
                if (destroyed) return;
                long now = SystemClock.elapsedRealtime();
                if (result != null) {
                    policy.observe(result.active, now);
                    presentAttention(result);
                    updateNotification(result.active > 0 ? "Working · safe to lock the screen"
                        : result.waiting > 0 ? "Waiting for your response · battery saving" : "Ready · battery saving");
                } else updateNotification("Connection interrupted · checking again");
                updateLock();
                if (policy.expired(now)) {
                    getSystemService(NotificationManager.class).notify(2, notification("Connection lost. Open OpenAIDE to recover.", "completion"));
                    stopSelf();
                } else if (!visible && result != null && result.active == 0 && now >= settlingUntil) stopSelf();
                else handler.postDelayed(pollTask, result == null ? 10_000 : result.active > 0 || !visible ? 5_000 : 15_000);
            });
        });
    }

    /** Shows each new Task Attention Event that occurred out of sight and withdraws superseded ones. */
    private void presentAttention(ServerStatus status) {
        NotificationManager manager = getSystemService(NotificationManager.class);
        Map<String, String> shown = new HashMap<>();
        for (StatusBarNotification active : manager.getActiveNotifications()) {
            String tag = active.getTag();
            String eventId = active.getNotification().extras.getString("event_id");
            if (tag != null && tag.startsWith(TASK_TAG) && eventId != null) shown.put(tag.substring(TASK_TAG.length()), eventId);
        }
        java.util.List<String> stale = AttentionNotifications.stale(shown, status.attention);
        for (String taskId : stale) manager.cancel(TASK_TAG + taskId, TASK_NOTIFICATION);
        java.util.List<AttentionNotifications.Attention> show = attention.observe(status.attention, visible, hiddenSince);
        for (AttentionNotifications.Attention event : show) {
            manager.notify(TASK_TAG + event.taskId, TASK_NOTIFICATION, taskNotification(event));
            Log.i("OpenAIDE", "task_attention_notification outcome=shown reason=" + event.reason);
        }
        if (!stale.isEmpty()) Log.i("OpenAIDE", "task_attention_notification outcome=withdrawn count=" + stale.size());
        if (!show.isEmpty() && !manager.areNotificationsEnabled()) Log.w("OpenAIDE", "task_attention_notification outcome=blocked");
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
        protection.apply(policy.protect(SystemClock.elapsedRealtime(), settlingUntil));
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
        if (protection != null) protection.close();
        stopForeground(STOP_FOREGROUND_REMOVE);
        Log.i("OpenAIDE", "background_work outcome=stopped");
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
