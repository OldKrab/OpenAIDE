package io.openaide.android;

import android.annotation.SuppressLint;
import android.app.Activity;
import android.content.Intent;
import android.os.Bundle;
import android.webkit.JavascriptInterface;
import android.webkit.WebResourceRequest;
import android.webkit.WebResourceResponse;
import android.webkit.WebView;
import android.webkit.WebViewClient;
import android.widget.FrameLayout;
import java.io.ByteArrayInputStream;
import java.util.Collections;
import java.util.HashMap;
import org.json.JSONObject;

public final class SetupActivity extends Activity {
    private WebView page;
    private ConnectionController controller;
    private boolean loaded;

    @SuppressLint("SetJavaScriptEnabled")
    @Override public void onCreate(Bundle state) {
        super.onCreate(state);
        WebView.setWebContentsDebuggingEnabled((getApplicationInfo().flags & android.content.pm.ApplicationInfo.FLAG_DEBUGGABLE) != 0);
        controller = new ConnectionController(this, new ConnectionController.View() {
            @Override public void render(JSONObject state) {
                if (loaded && !isDestroyed()) page.evaluateJavascript("window.receive(" + state + ")", null);
            }
            @Override public void changed() { setResult(RESULT_OK); finish(); }
            @Override public void close() { finish(); }
            @Override public void scanned(String address) {
                if (loaded && !isDestroyed()) page.evaluateJavascript("window.scanned(" + JSONObject.quote(address) + ")", null);
            }
        }, true);
        page = new WebView(this);
        page.setBackgroundColor(0xfff8f9fb);
        page.getSettings().setJavaScriptEnabled(true);
        page.getSettings().setAllowFileAccess(false);
        page.getSettings().setAllowContentAccess(false);
        page.getSettings().setSaveFormData(false);
        page.setImportantForAutofill(android.view.View.IMPORTANT_FOR_AUTOFILL_NO_EXCLUDE_DESCENDANTS);
        page.addJavascriptInterface(new Bridge(), "OpenAIDESetup");
        page.setWebViewClient(new WebViewClient() {
            @Override public boolean shouldOverrideUrlLoading(WebView view, WebResourceRequest request) { return true; }
            @Override public WebResourceResponse shouldInterceptRequest(WebView view, WebResourceRequest request) {
                String asset = SetupPagePolicy.asset(request.getUrl().toString());
                try {
                    if (!"GET".equals(request.getMethod()) || asset == null) throw new java.io.IOException();
                    String type = asset.endsWith(".js") ? "text/javascript" : asset.endsWith(".css") ? "text/css" : "text/html";
                    var headers = new HashMap<String, String>();
                    headers.put("Content-Security-Policy", "default-src 'none'; script-src 'self'; style-src 'self'; img-src 'none'; connect-src 'none'; frame-src 'none'; object-src 'none'; base-uri 'none'; form-action 'none'");
                    headers.put("Cache-Control", "no-store");
                    return new WebResourceResponse(type, "UTF-8", 200, "OK", headers, getAssets().open(asset));
                } catch (java.io.IOException error) {
                    return new WebResourceResponse("text/plain", "UTF-8", 403, "Forbidden", Collections.emptyMap(), new ByteArrayInputStream(new byte[0]));
                }
            }
            @Override public void onPageFinished(WebView view, String url) {
                if (!(SetupPagePolicy.ORIGIN + "index.html").equals(url)) return;
                loaded = true;
                controller.resume();
            }
        });
        FrameLayout frame = new FrameLayout(this);
        frame.setOnApplyWindowInsetsListener((view, insets) -> {
            view.setPadding(insets.getSystemWindowInsetLeft(), insets.getSystemWindowInsetTop(),
                insets.getSystemWindowInsetRight(), insets.getSystemWindowInsetBottom());
            return insets;
        });
        frame.addView(page);
        setContentView(frame);
        page.loadUrl(SetupPagePolicy.ORIGIN + "index.html");
    }

    public final class Bridge {
        @JavascriptInterface public void send(String input) {
            if (input == null || input.length() > 8192) return;
            runOnUiThread(() -> {
                if (isDestroyed() || !loaded) return;
                try { controller.receive(new JSONObject(input)); }
                catch (Exception ignored) { }
            });
        }
    }

    @Override public void onRequestPermissionsResult(int request, String[] permissions, int[] results) {
        super.onRequestPermissionsResult(request, permissions, results);
        controller.onRequestPermissionsResult(request, permissions, results);
    }
    @Override protected void onActivityResult(int request, int result, Intent data) {
        super.onActivityResult(request, result, data);
        controller.onActivityResult(request, result, data);
    }
    @Override protected void onResume() {
        super.onResume();
        page.resumeTimers();
        page.onResume();
        controller.resume();
    }
    @Override protected void onPause() { page.onPause(); super.onPause(); }
    @Override public void onBackPressed() { page.evaluateJavascript("window.back()", null); }
    @Override protected void onDestroy() {
        loaded = false;
        controller.dispose();
        page.removeJavascriptInterface("OpenAIDESetup");
        page.destroy();
        super.onDestroy();
    }
}
