package io.openaide.android;

import android.os.PowerManager;
import android.util.Log;

final class WorkProtection implements AutoCloseable {
    private final PowerManager.WakeLock wakeLock;
    WorkProtection(PowerManager manager, String tag) {
        wakeLock = manager.newWakeLock(PowerManager.PARTIAL_WAKE_LOCK, tag);
        wakeLock.setReferenceCounted(false);
    }
    void apply(boolean active) {
        if (active) {
            if (!wakeLock.isHeld()) Log.i("OpenAIDE", "power_state state=active");
            wakeLock.acquire(150_000);
        } else close();
    }
    boolean isHeld() { return wakeLock.isHeld(); }
    @Override public void close() {
        if (wakeLock.isHeld()) {
            wakeLock.release();
            Log.i("OpenAIDE", "power_state state=idle");
        }
    }
}
