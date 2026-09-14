package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Notification;
import android.app.NotificationChannel;
import android.app.NotificationManager;
import android.app.PendingIntent;
import android.app.Service;
import android.content.Intent;
import android.os.IBinder;
import android.os.PowerManager;
import android.util.Log;

public final class BackgroundService extends Service {
    private PowerManager.WakeLock wakeLock;

    @SuppressLint("WakelockTimeout")
    @Override public void onCreate() {
        super.onCreate();
        NotificationManager manager = getSystemService(NotificationManager.class);
        manager.createNotificationChannel(new NotificationChannel(
            "background", "Background work", NotificationManager.IMPORTANCE_LOW));
        PendingIntent open = PendingIntent.getActivity(this, 0, new Intent(this, MainActivity.class),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent settings = PendingIntent.getActivity(this, 1,
            new Intent(this, MainActivity.class).putExtra("show_settings", true),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        PendingIntent stop = PendingIntent.getService(this, 2,
            new Intent(this, BackgroundService.class).setAction("stop"),
            PendingIntent.FLAG_UPDATE_CURRENT | PendingIntent.FLAG_IMMUTABLE);
        Notification notification = new Notification.Builder(this, "background")
            .setSmallIcon(R.drawable.ic_agent)
            .setContentTitle("OpenAIDE · background mode")
            .setContentText("Keeping this phone awake for your agents")
            .setContentIntent(open).setOngoing(true)
            .addAction(new Notification.Action.Builder(null, "Settings", settings).build())
            .addAction(new Notification.Action.Builder(null, "Turn off", stop).build()).build();
        startForeground(1, notification);
        wakeLock = getSystemService(PowerManager.class).newWakeLock(
            PowerManager.PARTIAL_WAKE_LOCK, "OpenAIDE:background-work");
        wakeLock.acquire();
        Log.i("OpenAIDE", "background_work outcome=started");
    }

    @Override public int onStartCommand(Intent intent, int flags, int startId) {
        if (intent != null && "stop".equals(intent.getAction())) {
            getSharedPreferences("connection", MODE_PRIVATE).edit().putBoolean("background", false).apply();
            stopSelf();
            return START_NOT_STICKY;
        }
        return START_STICKY;
    }

    @Override public void onDestroy() {
        if (wakeLock != null && wakeLock.isHeld()) wakeLock.release();
        stopForeground(STOP_FOREGROUND_REMOVE);
        Log.i("OpenAIDE", "background_work outcome=stopped");
        super.onDestroy();
    }

    @Override public IBinder onBind(Intent intent) { return null; }
}
