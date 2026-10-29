package io.openaide.android;

import android.content.Context;
import android.content.res.Configuration;

/**
 * The neutral colors of the shell's own screens, matching the setup screens: the
 * primary button is text-on-background inverted. They follow the phone's theme.
 */
final class ShellColors {
    private ShellColors() {}

    static int background(Context context) { return dark(context) ? 0xff1b1d22 : 0xfff6f7f9; }
    static int text(Context context) { return dark(context) ? 0xffeceef1 : 0xff23262d; }
    static int muted(Context context) { return dark(context) ? 0xff9da3ae : 0xff666d7a; }

    static int dp(Context context, int value) { return (int) (value * context.getResources().getDisplayMetrics().density); }

    private static boolean dark(Context context) {
        return (context.getResources().getConfiguration().uiMode & Configuration.UI_MODE_NIGHT_MASK) == Configuration.UI_MODE_NIGHT_YES;
    }
}
