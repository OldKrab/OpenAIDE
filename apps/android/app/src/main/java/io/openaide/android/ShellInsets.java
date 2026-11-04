package io.openaide.android;

import android.graphics.Insets;
import android.os.Build;
import android.view.View;
import android.view.WindowInsets;

/**
 * The shell frame owns the system bars, display cutout, and software keyboard: it pads the WebView
 * clear of them. Since WebView 136 the page also reads those insets as env(safe-area-inset-*), so
 * the frame passes them on zeroed, or the page would add the same space a second time (the
 * Composer's bottom padding, the phone Settings headers). Zeroed rather than consumed, so the
 * WebView still sees every change and never keeps a stale inset after the keyboard hides.
 */
final class ShellInsets {
    private ShellInsets() {}

    static void padFrame(View frame) {
        frame.setOnApplyWindowInsetsListener((view, insets) -> {
            if (Build.VERSION.SDK_INT >= 30) {
                int types = WindowInsets.Type.systemBars() | WindowInsets.Type.displayCutout() | WindowInsets.Type.ime();
                Insets used = insets.getInsets(types);
                view.setPadding(used.left, used.top, used.right, used.bottom);
                return new WindowInsets.Builder(insets).setInsets(types, Insets.NONE).build();
            }
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            WindowInsets zeroed = insets.replaceSystemWindowInsets(0, 0, 0, 0);
            return Build.VERSION.SDK_INT >= 28 ? zeroed.consumeDisplayCutout() : zeroed;
        });
    }
}
