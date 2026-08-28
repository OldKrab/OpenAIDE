use objc2_app_kit::{
    NSView, NSViewLayerContentsPlacement, NSViewLayerContentsRedrawPolicy, NSWindow,
};

/// Keeps AppKit in charge of window zoom while allowing it to scale the cached
/// webview layers until WebKit performs the final viewport layout.
pub fn configure(window: &tauri::WebviewWindow) -> tauri::Result<()> {
    let started_at = std::time::Instant::now();
    eprintln!("desktop_webview_resize_presentation_started");
    let result = window.with_webview(|webview| unsafe {
        let webview: &NSView = &*webview.inner().cast();
        configure_view(webview);
        if let Some(parent) = webview.superview() {
            configure_view(&parent);
            if let Some(theme_frame) = parent.superview() {
                theme_frame.setLayerContentsRedrawPolicy(
                    NSViewLayerContentsRedrawPolicy::DuringViewResize,
                );
            }
        }
    });
    match result {
        Ok(()) => {
            eprintln!(
                "desktop_webview_resize_presentation_completed outcome=success duration_ms={}",
                started_at.elapsed().as_millis()
            );
            Ok(())
        }
        Err(error) => {
            eprintln!(
                "desktop_webview_resize_presentation_completed outcome=failure error_kind=native_view_configuration duration_ms={}",
                started_at.elapsed().as_millis()
            );
            Err(error)
        }
    }
}

fn configure_view(view: &NSView) {
    view.setWantsLayer(true);
    view.setLayerContentsRedrawPolicy(NSViewLayerContentsRedrawPolicy::DuringViewResize);
    view.setLayerContentsPlacement(NSViewLayerContentsPlacement::ScaleAxesIndependently);
}

/// Prepares WKWebView at AppKit's native standard-frame size before `zoom:`
/// starts. AppKit otherwise withholds that resize until its animation ends.
pub fn prepare_native_zoom(window: &tauri::WebviewWindow) -> Result<(), String> {
    let started_at = std::time::Instant::now();
    eprintln!("desktop_native_zoom_prepare_started");
    window
        .with_webview(move |platform_webview| unsafe {
            let webview: &NSView = &*platform_webview.inner().cast();
            let native_window: &NSWindow = &*platform_webview.ns_window().cast();
            if !native_window.isZoomed()
                && let Some(screen) = native_window.screen()
            {
                let content_size = native_window
                    .contentRectForFrameRect(screen.visibleFrame())
                    .size;
                if let Some(parent) = webview.superview() {
                    parent.setFrameSize(content_size);
                    parent.layoutSubtreeIfNeeded();
                }
                webview.setFrameSize(content_size);
                webview.layoutSubtreeIfNeeded();
            }
            eprintln!(
                "desktop_native_zoom_prepare_completed outcome=success duration_ms={}",
                started_at.elapsed().as_millis()
            );
        })
        .map_err(|_| "desktop native zoom could not prepare the webview".to_string())
}

/// Runs AppKit's real zoom action with every native layer configured to redraw
/// during the resize animation.
pub fn perform_native_zoom(window: &tauri::WebviewWindow) -> Result<(), String> {
    let started_at = std::time::Instant::now();
    eprintln!("desktop_native_zoom_perform_started");
    window
        .with_webview(move |platform_webview| unsafe {
            let native_window: &NSWindow = &*platform_webview.ns_window().cast();
            native_window.zoom(None);
            eprintln!(
                "desktop_native_zoom_perform_completed outcome=success duration_ms={}",
                started_at.elapsed().as_millis()
            );
        })
        .map_err(|_| "desktop native zoom could not run AppKit zoom".to_string())
}
