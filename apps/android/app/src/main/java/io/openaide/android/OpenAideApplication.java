package io.openaide.android;

import android.app.Application;

/** Owns what must exist before any screen or service of the process runs. */
public final class OpenAideApplication extends Application {
    @Override public void onCreate() {
        super.onCreate();
        AndroidDiagnostics.install(this);
        // Marks where Android restarted the app, which separates a cold start from a return to a live process.
        new AndroidDiagnostics().record("app_process", "started", 0, 0);
    }
}
