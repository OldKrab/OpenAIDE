//! Native Quit: the open-Terminal warning and the fallback that guarantees exit.
//!
//! The WebView owns the graceful path (detach, close the session, end the App Server hold);
//! the fallback ends the process natively when that path cannot finish in time. Quitting a
//! Desktop that owns its App Server stops it, which closes every Terminal (ADR-0071), so a
//! Quit with Terminals open asks first. The question comes before the fallback starts, so
//! the user's answer never spends the exit budget. The count is pushed by the WebView as its
//! Terminal list changes, so asking needs no request to anything that might be hung.

use std::sync::atomic::{AtomicBool, AtomicU32, Ordering};
use std::thread;
use std::time::Duration;

use tauri::{Emitter, Manager};

/// The WebView's graceful Quit is bounded by 3 s to detach plus 2 s to close its session.
const QUIT_FALLBACK: Duration = Duration::from_secs(5);

#[derive(Default)]
pub(crate) struct DesktopQuitState {
    // Native close can arrive before the WebView has registered its command
    // listener. The WebView consumes this latch after crossing that boundary.
    pending: AtomicBool,
    /// Open Terminals of this Desktop's App Server, as last reported by the WebView.
    open_terminals: AtomicU32,
    /// Keeps a second Quit from stacking another question while one is shown.
    confirming: AtomicBool,
}

#[tauri::command]
pub(crate) fn desktop_take_pending_quit(state: tauri::State<'_, DesktopQuitState>) -> bool {
    state.pending.swap(false, Ordering::AcqRel)
}

#[tauri::command]
pub(crate) fn desktop_set_open_terminals(state: tauri::State<'_, DesktopQuitState>, count: u32) {
    state.open_terminals.store(count, Ordering::Release);
}

/// The question to ask before Quit, if quitting would close Terminals.
pub(crate) fn quit_warning(open_terminals: u32, owns_app_server: bool) -> Option<String> {
    // A paired or SSH Computer's App Server outlives this Desktop, and its Terminals with it.
    // TODO(terminals): a Remote Device still connected also keeps this App Server running, so
    // the warning overstates; App Server would have to report whether this is its last client.
    if !owns_app_server || open_terminals == 0 {
        return None;
    }
    Some(match open_terminals {
        1 => "1 terminal will close.".to_string(),
        count => format!("{count} terminals will close."),
    })
}

/// Quit from the native menu, or the window's close on Windows.
pub(crate) fn request_quit(app: &tauri::AppHandle) {
    let state = app.state::<DesktopQuitState>();
    if state.pending.load(Ordering::Acquire) {
        // A repeated Quit is the WebView's force-exit request.
        let _ = app.emit("desktop-command", "quit");
        return;
    }
    let warning = quit_warning(
        state.open_terminals.load(Ordering::Acquire),
        // This computer's App Server is in use unless the window moved to another.
        !matches!(
            app.state::<crate::desktop_connection::DesktopConnections>()
                .computer_in_use(),
            Some(Some(_))
        ),
    );
    let Some(warning) = warning else {
        start_quit(app);
        return;
    };
    if state.confirming.swap(true, Ordering::AcqRel) {
        return;
    }
    let count = state.open_terminals.load(Ordering::Acquire);
    desktop_log!("desktop_quit_confirm_shown open_terminals={count}");
    use tauri_plugin_dialog::{DialogExt, MessageDialogButtons, MessageDialogKind};
    let handle = app.clone();
    app.dialog()
        .message(warning)
        .title("Quit OpenAIDE?")
        .kind(MessageDialogKind::Warning)
        .buttons(MessageDialogButtons::OkCancelCustom(
            "Quit".to_string(),
            "Cancel".to_string(),
        ))
        .show(move |quit| {
            handle
                .state::<DesktopQuitState>()
                .confirming
                .store(false, Ordering::Release);
            desktop_log!(
                "desktop_quit_confirm_answered outcome={}",
                if quit { "quit" } else { "cancelled" }
            );
            if quit {
                start_quit(&handle);
            }
        });
}

/// Hands Quit to the WebView and ends the process natively if it does not finish in time.
fn start_quit(app: &tauri::AppHandle) {
    if app
        .state::<DesktopQuitState>()
        .pending
        .swap(true, Ordering::AcqRel)
    {
        return;
    }
    let _ = app.emit("desktop-command", "quit");
    let app = app.clone();
    thread::spawn(move || {
        thread::sleep(QUIT_FALLBACK);
        if app
            .state::<DesktopQuitState>()
            .pending
            .swap(false, Ordering::AcqRel)
        {
            // Startup may not have produced a frontend session that can detach. Never
            // leave an unclosable native app behind.
            desktop_log!("desktop_quit_fallback_exit");
            app.exit(0);
        }
    });
}

#[cfg(test)]
#[path = "desktop_quit_tests.rs"]
mod tests;
