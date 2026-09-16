package io.openaide.android;

import android.content.Intent;
import android.os.Handler;
import android.os.Looper;
import android.test.InstrumentationTestCase;
import android.webkit.WebView;
import java.util.concurrent.CountDownLatch;
import java.util.concurrent.TimeUnit;

public final class HeaderChecks extends InstrumentationTestCase {
    public void testFilesActionStaysInTheTitleRow() throws Exception {
        checkTaskSurface(false);
    }

    public void testMobileComposer() throws Exception {
        checkTaskSurface(true);
    }

    public void testMobileHeaderSpacing() throws Exception {
        checkTaskSurface(true);
    }

    public void testRecoveryLayout() throws Exception {
        checkTaskSurface(true, true);
    }

    private void checkTaskSurface(boolean composerOnly) throws Exception {
        checkTaskSurface(composerOnly, false);
    }

    private void checkTaskSurface(boolean composerOnly, boolean recoveryLayout) throws Exception {
        var context = getInstrumentation().getTargetContext();
        MainActivity activity = (MainActivity) getInstrumentation().startActivitySync(new Intent(context, MainActivity.class).addFlags(Intent.FLAG_ACTIVITY_NEW_TASK));
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
            assertNotNull("Workspace did not start", browser[0]);
            WebView page = browser[0];
            getInstrumentation().runOnMainSync(page::resumeTimers);
            waitFor(page, "Boolean(document.querySelector('.task-open'))");
            run(page, "document.querySelector('.task-open').click()");
            waitFor(page, "Boolean(document.querySelector('.mobile-workbench-bar .project-files-entry'))");
            waitFor(page, "Boolean(document.querySelector('.mobile-workbench-bar .user-message-picker-trigger'))");
            assertEqualHeaderSpacing(page);
            checkMobileComposer(page);
            if (recoveryLayout) {
                checkRecoveryLayout(page);
                return;
            }
            if (composerOnly) return;
            waitFor(page, "(() => { const button=document.querySelector('.mobile-workbench-bar .project-files-entry'); const bar=button.closest('header'); const rect=button.getBoundingClientRect(); const bounds=bar.getBoundingClientRect(); return rect.width>=44 && rect.height>=44 && rect.top>=bounds.top && rect.bottom<=bounds.bottom && rect.right<=innerWidth && button.textContent==='' && getComputedStyle(document.querySelector('.task-work-stack-header')).display==='none'; })()");
            waitFor(page, "(() => { const button=document.querySelector('.mobile-workbench-bar .user-message-picker-trigger'); if(!button) return false; const rect=button.getBoundingClientRect(); const chat=document.querySelector('.message-list').getBoundingClientRect(); return rect.width>=44 && rect.height>=44 && rect.bottom<=chat.top && rect.right<=innerWidth && !document.querySelector('.message-list-shell .user-message-navigator'); })()");
            run(page, "document.querySelector('.user-message-picker-trigger').click()");
            waitFor(page, "(() => { const panel=document.querySelector('.user-message-picker'); if(!panel) return false; const rect=panel.getBoundingClientRect(); return rect.width>=240 && rect.right<=innerWidth && rect.left>=0 && rect.bottom<=innerHeight && panel.querySelectorAll('.user-message-picker-list button').length>1; })()");
            run(page, "document.querySelector('.user-message-picker-list button[title]').click()");
            waitFor(page, "document.querySelector('.user-message-picker-trigger').getAttribute('aria-expanded')==='false'");
            run(page, "document.querySelector('.user-message-picker-trigger').click()");
            waitFor(page, "document.querySelector('.user-message-picker-trigger').getAttribute('aria-expanded')==='true'");
            run(page, "window.dispatchEvent(new Event('openaide:back',{cancelable:true}))");
            waitFor(page, "document.querySelector('.user-message-picker-trigger').getAttribute('aria-expanded')==='false'");
            getInstrumentation().runOnMainSync(() -> page.getSettings().setUseWideViewPort(true));
            run(page, "window.checkViewport=document.querySelector('meta[name=viewport]').content; document.querySelector('meta[name=viewport]').content='width=1100';");
            waitFor(page, "innerWidth>=1000 && !document.querySelector('.mobile-workbench-bar .user-message-picker-trigger') && document.querySelector('.user-message-navigator') && getComputedStyle(document.querySelector('.user-message-navigator')).display!=='none' && getComputedStyle(document.querySelector('.user-message-navigation-entry')).display==='none'");
            waitFor(page, "Boolean(document.querySelector('.context-usage-edge')) && !document.querySelector('.context-usage-compact')");
            run(page, "document.querySelector('.message-list-shell').style.width='500px'");
            waitFor(page, "(() => { const entry=document.querySelector('.user-message-navigation-entry'); const chat=document.querySelector('.message-list'); return getComputedStyle(document.querySelector('.user-message-navigator')).display==='none' && entry.getBoundingClientRect().height>=44 && entry.getBoundingClientRect().top>=chat.getBoundingClientRect().bottom; })()");
            run(page, "document.querySelector('.user-message-picker-trigger').click()");
            waitFor(page, "document.querySelector('.user-message-picker-trigger').getAttribute('aria-expanded')==='true'");
            run(page, "window.dispatchEvent(new Event('openaide:back',{cancelable:true})); document.querySelector('.message-list-shell').style.width=''; document.querySelector('meta[name=viewport]').content=window.checkViewport; delete window.checkViewport;");
            getInstrumentation().runOnMainSync(() -> page.getSettings().setUseWideViewPort(false));
            waitFor(page, "Boolean(document.querySelector('.mobile-workbench-bar .user-message-picker-trigger'))");
            run(page, "document.querySelector('.mobile-workbench-bar .project-files-entry').click()");
            waitFor(page, "document.querySelector('.mobile-workbench-bar .project-files-entry').getAttribute('aria-expanded')==='true' && document.querySelector('.project-file-workspace:not([hidden])').getBoundingClientRect().height>100");
            waitFor(page, "!document.querySelector('.mobile-workbench-bar .user-message-picker-trigger')");
        } finally {
            getInstrumentation().runOnMainSync(activity::finish);
        }
    }

    private void checkMobileComposer(WebView page) throws Exception {
        waitFor(page, "(() => { const composer=document.querySelector('.composer'); const chip=composer?.querySelector('.context-usage-compact'); if(!chip) return false; const rect=composer.getBoundingClientRect(); const control=chip.getBoundingClientRect(); const actions=composer.querySelector('.composer-actions').getBoundingClientRect(); const options=composer.querySelector('.composer-controls').getBoundingClientRect(); return rect.left>=12 && innerWidth-rect.right>=12 && innerHeight-rect.bottom>=0 && innerHeight-rect.bottom<=12 && control.width>=44 && control.height>=44 && control.left>=rect.left && actions.right<=rect.right && options.right<=actions.left && !document.querySelector('.context-usage-edge'); })()");
        run(page, "document.querySelector('.context-usage-compact').click()");
        waitFor(page, "(() => { const panel=document.querySelector('.context-usage-popup'); if(!panel) return false; const rect=panel.getBoundingClientRect(); const composer=document.querySelector('.composer').getBoundingClientRect(); return rect.left>=0 && rect.right<=innerWidth && rect.top>=0 && rect.bottom<=composer.top && panel.querySelectorAll('[role=dialog]').length===0; })()");
        run(page, "window.dispatchEvent(new Event('openaide:back',{cancelable:true}))");
        waitFor(page, "document.querySelector('.context-usage-compact').getAttribute('aria-expanded')==='false'");
    }

    private void checkRecoveryLayout(WebView page) throws Exception {
        run(page, "window.recoveryFixture=document.createElement('section'); recoveryFixture.style.cssText='position:fixed;inset:0;z-index:99999;background:var(--oa-bg);padding:12px;display:flex;flex-direction:column;align-items:center;gap:12px'; const composer=document.querySelector('.composer').cloneNode(true); composer.querySelectorAll('.composer-footer-status').forEach(node=>node.remove()); const status=document.createElement('span'); status.className='composer-footer-status'; status.innerHTML='<strong>Reconnecting</strong><small>Draft saved</small>'; composer.querySelector('.composer-footer').insertBefore(status,composer.querySelector('.composer-actions')); const notice=document.createElement('div'); notice.className='task-connection-notice'; notice.innerHTML='<span>Unable to refresh task.</span><small>App Server instance changed while replacing an expired HTTP session</small><button type=button>Retry</button>'; recoveryFixture.append(notice,composer); document.body.append(recoveryFixture)");
        try {
            assertRecoveryGeometry(page, true);
            getInstrumentation().runOnMainSync(() -> page.getSettings().setUseWideViewPort(true));
            run(page, "window.recoveryViewport=document.querySelector('meta[name=viewport]').content; document.querySelector('meta[name=viewport]').content='width=1100'");
            waitFor(page, "innerWidth>=1000");
            assertRecoveryGeometry(page, false);
        } finally {
            run(page, "recoveryFixture.remove(); if(window.recoveryViewport) document.querySelector('meta[name=viewport]').content=recoveryViewport");
            getInstrumentation().runOnMainSync(() -> page.getSettings().setUseWideViewPort(false));
        }
    }

    private void assertRecoveryGeometry(WebView page, boolean narrow) throws Exception {
        String expression = "(() => { const root=recoveryFixture; const bounds=selector=>root.querySelector(selector).getBoundingClientRect(); const controls=bounds('.composer-controls'), actions=bounds('.composer-actions'), status=bounds('.composer-footer-status'), composer=bounds('.composer'), notice=bounds('.task-connection-notice'), button=bounds('.task-connection-notice button'), message=bounds('.task-connection-notice small'); return Math.abs(controls.top+controls.height/2-actions.top-actions.height/2)<2 && controls.right<=actions.left+1 && actions.right<=composer.right && notice.left>=0 && notice.right<=innerWidth && message.right<=button.left && " + (narrow ? "status.top>=actions.bottom && status.width>composer.width/2 && button.height>=44" : "status.right<=actions.left+1") + "; })()";
        waitFor(page, expression);
    }

    private void assertEqualHeaderSpacing(WebView page) throws Exception {
        CountDownLatch complete = new CountDownLatch(1);
        boolean[] aligned = {false};
        getInstrumentation().runOnMainSync(() -> page.evaluateJavascript(
            "(() => { const header=document.querySelector('.mobile-workbench-bar'); const buttons=['.project-files-entry','.user-message-picker-trigger','.task-permission-policy-trigger'].map(selector=>header.querySelector(selector)); if(buttons.some(button=>!button)) return false; const boxes=buttons.map(button=>button.getBoundingClientRect()); const icons=buttons.map(button=>button.querySelector('svg').getBoundingClientRect()); const center=rect=>rect.left+rect.width/2; const spacing=Math.abs((center(boxes[1])-center(boxes[0]))-(center(boxes[2])-center(boxes[1]))); return spacing<1 && boxes.every(rect=>rect.width>=44 && rect.height>=44) && icons.every(rect=>Math.abs(rect.width-icons[0].width)<1 && Math.abs(rect.height-icons[0].height)<1 && Math.abs(rect.top+rect.height/2-icons[0].top-icons[0].height/2)<1); })()",
            result -> { aligned[0] = "true".equals(result); complete.countDown(); }));
        assertTrue("Header measurement timed out", complete.await(10, TimeUnit.SECONDS));
        assertTrue("Header actions must have equal center spacing and icon sizes", aligned[0]);
    }

    private void run(WebView page, String script) throws Exception {
        CountDownLatch complete = new CountDownLatch(1);
        getInstrumentation().runOnMainSync(() -> page.evaluateJavascript(script, result -> complete.countDown()));
        assertTrue("Renderer command timed out", complete.await(10, TimeUnit.SECONDS));
    }

    private void waitFor(WebView page, String expression) throws Exception {
        CountDownLatch complete = new CountDownLatch(1);
        Handler handler = new Handler(Looper.getMainLooper());
        handler.post(new Runnable() {
            @Override public void run() {
                page.evaluateJavascript(expression, result -> {
                    if ("true".equals(result)) complete.countDown();
                    else handler.postDelayed(this, 50);
                });
            }
        });
        try { assertTrue("Header check timed out", complete.await(15, TimeUnit.SECONDS)); }
        finally { handler.removeCallbacksAndMessages(null); }
    }

}
