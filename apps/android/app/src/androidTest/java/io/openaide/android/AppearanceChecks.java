package io.openaide.android;

import android.content.Intent;
import android.graphics.Color;
import android.graphics.drawable.ColorDrawable;
import android.test.InstrumentationTestCase;
import android.view.View;
import android.webkit.WebView;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;
import org.json.JSONArray;

public final class AppearanceChecks extends InstrumentationTestCase {
    public void testSystemBarsFollowTheAppThemeWithoutChangingInsets() throws Exception {
        var context = getInstrumentation().getTargetContext();
        MainActivity activity = (MainActivity) getInstrumentation().startActivitySync(
            new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
        var field = MainActivity.class.getDeclaredField("browser");
        field.setAccessible(true);
        WebView[] browser = new WebView[1];
        try {
            long deadline = System.currentTimeMillis() + 15000;
            while (browser[0] == null && System.currentTimeMillis() < deadline) {
                getInstrumentation().runOnMainSync(() -> {
                    try { browser[0] = (WebView) field.get(activity); }
                    catch (Exception error) { throw new AssertionError(error); }
                });
                Thread.sleep(50);
            }
            assertNotNull(browser[0]);
            WebView page = browser[0];
            getInstrumentation().runOnMainSync(page::resumeTimers);
            waitFor(page, "Boolean(document.querySelector('.task-open'))");
            run(page, "document.querySelector('.task-open').click()");
            waitFor(page, "Boolean(document.querySelector('.mobile-workbench-bar'))");
            waitFor(page, "Boolean(document.querySelector('.message-list'))");
            assertEquals("Chat must scroll directly up to the header", "true", run(page,
                "(() => { const header=document.querySelector('.mobile-workbench-bar').getBoundingClientRect(); const chat=document.querySelector('.message-list').getBoundingClientRect(); return Math.abs(chat.top-header.bottom)<1; })()"));
            run(page, "window.appearanceCheckTheme=document.body.dataset.theme");
            checkTheme(activity, page, "dark");
            checkTheme(activity, page, "light");
            run(page, "document.body.dataset.theme=window.appearanceCheckTheme; delete window.appearanceCheckTheme");
        } finally {
            getInstrumentation().runOnMainSync(activity::finish);
        }
    }

    private void checkTheme(MainActivity activity, WebView page, String theme) throws Exception {
        int[] padding = new int[2];
        getInstrumentation().runOnMainSync(() -> {
            View frame = (View) page.getParent();
            padding[0] = frame.getPaddingTop();
            padding[1] = frame.getPaddingBottom();
        });
        run(page, "document.body.dataset.theme='" + theme + "'");
        JSONArray channels = new JSONArray(run(page, "(() => { const canvas=document.createElement('canvas'); canvas.width=canvas.height=1; const context=canvas.getContext('2d'); context.fillStyle=getComputedStyle(document.querySelector('.mobile-workbench-bar')).backgroundColor; context.fillRect(0,0,1,1); return Array.from(context.getImageData(0,0,1,1).data); })()"));
        int expected = Color.rgb(channels.getInt(0), channels.getInt(1), channels.getInt(2));
        boolean[] matched = {false};
        long deadline = System.currentTimeMillis() + 10000;
        while (!matched[0] && System.currentTimeMillis() < deadline) {
            getInstrumentation().runOnMainSync(() -> {
                View frame = (View) page.getParent();
                int flags = activity.getWindow().getDecorView().getSystemUiVisibility();
                boolean darkStatusIcons = (flags & View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR) != 0;
                boolean darkNavigationIcons = (flags & View.SYSTEM_UI_FLAG_LIGHT_NAVIGATION_BAR) != 0;
                matched[0] = ((ColorDrawable) frame.getBackground()).getColor() == expected
                    && darkStatusIcons == theme.equals("light") && darkNavigationIcons == theme.equals("light");
                assertEquals(padding[0], frame.getPaddingTop());
                assertEquals(padding[1], frame.getPaddingBottom());
            });
            if (!matched[0]) Thread.sleep(50);
        }
        assertTrue("Native bars did not follow " + theme + " app theme", matched[0]);
        assertEquals("true", run(page, "(() => { const header=getComputedStyle(document.querySelector('.mobile-workbench-bar')); return header.backgroundColor!==getComputedStyle(document.body).backgroundColor && header.borderBottomWidth==='0px'; })()"));
        getInstrumentation().runOnMainSync(() -> {
            var appearance = new SystemBarAppearance(activity, page);
            assertFalse(appearance.apply("transparent"));
            assertFalse(appearance.apply("#ffffffff"));
            assertFalse(appearance.apply(null));
            assertEquals(expected, ((ColorDrawable) ((View) page.getParent()).getBackground()).getColor());
        });
    }

    private String run(WebView page, String script) throws Exception {
        CountDownLatch complete = new CountDownLatch(1);
        String[] value = new String[1];
        getInstrumentation().runOnMainSync(() -> page.evaluateJavascript(script, result -> {
            value[0] = result;
            complete.countDown();
        }));
        assertTrue("Appearance renderer timed out", complete.await(10, TimeUnit.SECONDS));
        return value[0];
    }

    private void waitFor(WebView page, String expression) throws Exception {
        long deadline = System.currentTimeMillis() + 15000;
        while (System.currentTimeMillis() < deadline) {
            if ("true".equals(run(page, expression))) return;
            Thread.sleep(50);
        }
        fail("Appearance page did not become ready");
    }
}
