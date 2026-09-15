package io.openaide.android;

import android.app.Activity;
import android.graphics.Color;
import android.os.Build;
import android.view.View;
import android.webkit.WebView;

final class SystemBarAppearance {
    private final Activity activity;
    private final WebView browser;
    private final AndroidDiagnostics diagnostics = new AndroidDiagnostics();
    private Integer applied;

    SystemBarAppearance(Activity activity, WebView browser) {
        this.activity = activity;
        this.browser = browser;
    }

    boolean apply(String value) {
        if (value == null || !value.matches("#[0-9a-fA-F]{6}")) return false;
        int color = Color.parseColor(value);
        if (applied != null && applied == color) return true;
        diagnostics.record("system_appearance", "started", 0, 0);
        var window = activity.getWindow();
        window.setStatusBarColor(color);
        window.setNavigationBarColor(color);
        if (Build.VERSION.SDK_INT >= 29) {
            window.setStatusBarContrastEnforced(false);
            window.setNavigationBarContrastEnforced(false);
        }
        View decor = window.getDecorView();
        int lightIcons = View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR | View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR;
        int flags = decor.getSystemUiVisibility() & ~lightIcons;
        if (Color.luminance(color) > 0.5f) flags |= lightIcons;
        decor.setSystemUiVisibility(flags);
        if (browser.getParent() instanceof View frame) frame.setBackgroundColor(color);
        browser.setBackgroundColor(color);
        applied = color;
        diagnostics.record("system_appearance", "applied", 0, 0);
        return true;
    }
}
