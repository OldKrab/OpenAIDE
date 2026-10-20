package io.openaide.android;

import android.content.BroadcastReceiver;
import android.content.Context;
import android.content.Intent;

/** Receives the package installer's status for an update session AppUpdates committed. */
public final class AppUpdateReceiver extends BroadcastReceiver {
    @Override public void onReceive(Context context, Intent intent) {
        AppUpdates.get(context).installStatus(intent);
    }
}
