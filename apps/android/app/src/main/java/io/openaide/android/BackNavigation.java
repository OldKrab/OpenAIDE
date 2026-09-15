package io.openaide.android;

import android.app.Activity;
import android.os.Build;
import android.os.Handler;
import android.os.Looper;
import android.os.SystemClock;
import android.webkit.WebView;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;
import java.util.function.Supplier;

final class BackNavigation {
    private final Activity activity;
    private final Supplier<WebView> browser;
    private final Runnable fallback;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private final Handler handler = new Handler(Looper.getMainLooper());
    private OnBackInvokedCallback callback;
    private int sequence;
    private boolean pending;

    BackNavigation(Activity activity, Supplier<WebView> browser, Runnable fallback) {
        this.activity = activity;
        this.browser = browser;
        this.fallback = fallback;
        if (Build.VERSION.SDK_INT >= 33) {
            callback = this::back;
            activity.getOnBackInvokedDispatcher().registerOnBackInvokedCallback(OnBackInvokedDispatcher.PRIORITY_DEFAULT, callback);
        }
    }

    void back() {
        if (pending) return;
        WebView page = browser.get();
        if (page == null) { fallback.run(); return; }
        pending = true;
        int request = ++sequence;
        long started = SystemClock.elapsedRealtime();
        diagnostics.record("back_navigation", "started", request, 0);
        handler.postDelayed(() -> {
            if (!pending || request != sequence) return;
            pending = false;
            sequence++;
            diagnostics.record("back_navigation", "timeout", request, SystemClock.elapsedRealtime() - started);
            activity.moveTaskToBack(true);
        }, 1500);
        page.evaluateJavascript("window.dispatchEvent(new Event('openaide:back', {cancelable:true}))", result -> {
            if (request != sequence || activity.isDestroyed()) return;
            pending = false;
            handler.removeCallbacksAndMessages(null);
            if (browser.get() != page) return;
            boolean handled = "false".equals(result);
            diagnostics.record("back_navigation", handled ? "handled" : "history", request, SystemClock.elapsedRealtime() - started);
            if (!handled) fallback.run();
        });
    }

    void dispose() {
        sequence++;
        handler.removeCallbacksAndMessages(null);
        if (Build.VERSION.SDK_INT >= 33) activity.getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback(callback);
    }
}
