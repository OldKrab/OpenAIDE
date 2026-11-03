package io.openaide.android;

import android.annotation.TargetApi;
import android.os.Build;
import android.os.SystemClock;
import android.view.View;
import android.view.WindowInsets;
import android.webkit.WebView;

/**
 * Tells the page when Android's software keyboard closes. Android hides it on Back or its own
 * hide key but leaves the page's text field focused, and the page cannot tell from its viewport:
 * that also changes while the keyboard opens on some phones. Android's own IME state is the one
 * reliable source. The frame's inset listener misses it where adjustResize still resizes the
 * window (before Android 15), so every layout reads the root insets instead. Android 10 and older
 * report no IME state; there the page keeps its focus, as it always did.
 */
@TargetApi(30)
final class SoftwareKeyboard {
    static final String HIDDEN_EVENT = "openaide:keyboardHidden";

    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private final View frame;
    private final WebView browser;
    private boolean shown;
    private long shownAt;
    private int transitions;

    private SoftwareKeyboard(View frame, WebView browser) {
        this.frame = frame;
        this.browser = browser;
    }

    static void watch(View frame, WebView browser) {
        if (Build.VERSION.SDK_INT < 30) return;
        SoftwareKeyboard keyboard = new SoftwareKeyboard(frame, browser);
        frame.getViewTreeObserver().addOnGlobalLayoutListener(keyboard::laidOut);
    }

    private void laidOut() {
        WindowInsets insets = frame.getRootWindowInsets();
        if (insets == null) return;
        boolean visible = insets.isVisible(WindowInsets.Type.ime());
        if (visible == shown) return;
        shown = visible;
        transitions++;
        long now = SystemClock.elapsedRealtime();
        // How long it stayed open tells a keyboard that flashed closed from one the user dismissed.
        diagnostics.record("software_keyboard", visible ? "shown" : "hidden", transitions, visible ? 0 : now - shownAt);
        if (visible) { shownAt = now; return; }
        browser.evaluateJavascript("window.dispatchEvent(new Event('" + HIDDEN_EVENT + "'))", null);
    }
}
