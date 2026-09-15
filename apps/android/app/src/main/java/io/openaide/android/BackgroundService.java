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
import android.util.Log;
import java.util.concurrent.ExecutorService;
import java.util.concurrent.Executors;

public final class BackgroundService extends Service {
    private WorkProtection protection;
    private final Handler handler = new Handler(Looper.getMainLooper());
    private final ExecutorService worker = Executors.newSingleThreadExecutor();
    private final PowerPolicy policy = new PowerPolicy(SystemClock.elapsedRealtime());
    private boolean visible;
    private boolean destroyed;
    private boolean polling;
    private final Runnable pollTask = this::poll;
    private long settlingUntil;
    private int previousActive;
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
            visible = intent.getBooleanExtra("visible", false);
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
            try { status = ServerStatus.read(new ConnectionStore(this).local()); }
            catch (Exception ignored) { }
            ServerStatus result = status;
            handler.post(() -> {
                polling = false;
                if (destroyed) return;
                long now = SystemClock.elapsedRealtime();
                if (result != null) {
                    policy.observe(result.active, now);
                    if (previousActive > 0 && result.active == 0 && !visible) {
                        getSystemService(NotificationManager.class).notify(2,
                            notification(result.waiting > 0 ? "Your agent needs your attention" : "Agent work ended. Open to review the result.", "completion"));
                    }
                    previousActive = result.active;
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
