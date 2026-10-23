//! Watches this computer's own Tasks for attention while the window uses a
//! paired computer's App Server (ADR-0070 Attention).
//!
//! The window's notifications follow the App Server it renders, so without this
//! a Task here that needs the user would go unnoticed while another computer is
//! in use. A native thread holds `taskAttention/wait` on this computer's App
//! Server, which the Desktop keeps running, and hands each changed list to the
//! window; the window's notification manager applies the usual baseline,
//! in-sight, and one-per-Task rules. The watch runs only while a paired computer
//! is in use: with this computer in use the window's own replica already covers
//! it. It initializes as a shell-hold client, so it never blocks a Desktop Update.

use std::sync::Mutex;
use std::thread;
use std::time::{Duration, Instant};

use serde_json::{Value, json};
use tauri::{Emitter, Manager};

use crate::desktop_app_server_hold::{HoldError, HoldTarget, post_loopback};
use crate::desktop_connection::DesktopConnections;

/// The App Server holds a wait for up to 25 s; the read allows for that and a slow answer.
const WAIT_TIMEOUT: Duration = Duration::from_secs(40);
const REQUEST_TIMEOUT: Duration = Duration::from_secs(5);
/// How often an idle watch looks whether a paired computer came into use.
const IDLE_INTERVAL: Duration = Duration::from_secs(2);
const RETRY_DELAYS: [Duration; 5] = [
    Duration::from_secs(1),
    Duration::from_secs(2),
    Duration::from_secs(5),
    Duration::from_secs(15),
    Duration::from_secs(30),
];
pub(crate) const ATTENTION_EVENT: &str = "desktop-local-attention";

/// The latest attention list, kept for a window that reloads mid-watch.
#[derive(Default)]
pub(crate) struct LocalAttention {
    latest: Mutex<Option<Value>>,
}

/// One answer of the watch loop.
#[derive(Debug, PartialEq)]
pub(crate) enum Step {
    /// No paired computer is in use; nothing to watch.
    Idle,
    /// The attention list changed (or is the first one); the window should know.
    Changed(Value),
    /// The held wait ended with nothing new.
    Unchanged,
    /// A request failed; try again after the delay.
    Retry(Duration),
}

/// The watch's state between requests, kept apart from Tauri so it can be tested.
pub(crate) struct Watch {
    client_instance_id: String,
    target: Option<HoldTarget>,
    initialized: bool,
    cursor: Option<String>,
    failures: usize,
    next_request: u64,
    started: Option<Instant>,
}

impl Watch {
    pub(crate) fn new() -> Self {
        Self {
            client_instance_id: format!("desktop-attention-{}", uuid::Uuid::new_v4()),
            target: None,
            initialized: false,
            cursor: None,
            failures: 0,
            next_request: 0,
            started: None,
        }
    }

    /// Advances the watch for `target` (this computer's App Server while a paired
    /// computer is in use, else `None`) with `post` as the transport.
    pub(crate) fn step(
        &mut self,
        target: Option<HoldTarget>,
        post: &mut impl FnMut(
            &HoldTarget,
            &str,
            &str,
            &str,
            Value,
            Duration,
        ) -> Result<Value, HoldError>,
    ) -> Step {
        let Some(target) = target else {
            if let Some(started) = self.started.take() {
                desktop_log!(
                    "desktop_local_attention_stopped reason=this_computer_in_use duration_ms={}",
                    started.elapsed().as_millis()
                );
            }
            self.reset(None);
            return Step::Idle;
        };
        if self.target.as_ref() != Some(&target) {
            let replaced = self.target.is_some();
            self.reset(Some(target.clone()));
            if self.started.is_none() {
                self.started = Some(Instant::now());
            }
            desktop_log!("desktop_local_attention_started replaced={replaced}");
        }
        if !self.initialized {
            let params = json!({
                "clientInstanceId": self.client_instance_id,
                "shell": { "kind": "desktop", "name": "OpenAIDE", "version": env!("CARGO_PKG_VERSION") },
                "requestedSurface": { "kind": "home" },
                "workspaceRoots": [],
                "role": "shellHold",
            });
            match self.post(&target, "client/initialize", params, REQUEST_TIMEOUT, post) {
                Ok(_) => self.initialized = true,
                Err(error) => return self.failed("initialize", &error),
            }
        }
        let params = match &self.cursor {
            Some(cursor) => json!({ "cursor": cursor }),
            None => json!({}),
        };
        match self.post(&target, "taskAttention/wait", params, WAIT_TIMEOUT, post) {
            Ok(result) => {
                if self.failures > 0 {
                    desktop_log!(
                        "desktop_local_attention_recovered attempt_count={}",
                        self.failures
                    );
                }
                self.failures = 0;
                let cursor = result["cursor"].as_str().map(str::to_string);
                let first = self.cursor.is_none();
                let changed = first || cursor != self.cursor;
                self.cursor = cursor;
                let tasks = result.get("tasks").cloned().unwrap_or_else(|| json!([]));
                if !changed {
                    return Step::Unchanged;
                }
                let count = tasks.as_array().map_or(0, Vec::len);
                desktop_log!("desktop_local_attention_changed tasks={count} first={first}");
                Step::Changed(tasks)
            }
            Err(error) => {
                // A refusal is most often a client the App Server forgot; start over.
                if error == HoldError::Refused {
                    self.initialized = false;
                    self.cursor = None;
                }
                self.failed("wait", &error)
            }
        }
    }

    fn post(
        &mut self,
        target: &HoldTarget,
        method: &str,
        params: Value,
        timeout: Duration,
        post: &mut impl FnMut(
            &HoldTarget,
            &str,
            &str,
            &str,
            Value,
            Duration,
        ) -> Result<Value, HoldError>,
    ) -> Result<Value, HoldError> {
        self.next_request += 1;
        let id = format!("desktop-attention-{}", self.next_request);
        post(
            target,
            &self.client_instance_id,
            &id,
            method,
            params,
            timeout,
        )
    }

    fn failed(&mut self, stage: &str, error: &HoldError) -> Step {
        let delay = RETRY_DELAYS[self.failures.min(RETRY_DELAYS.len() - 1)];
        self.failures += 1;
        desktop_log!(
            "desktop_local_attention_failed stage={stage} error_class={} attempt_count={} retry_ms={}",
            error.class(),
            self.failures,
            delay.as_millis()
        );
        Step::Retry(delay)
    }

    fn reset(&mut self, target: Option<HoldTarget>) {
        self.target = target;
        self.initialized = false;
        self.cursor = None;
        self.failures = 0;
    }
}

/// Starts the watch thread for the rest of the process.
pub(crate) fn install(app: &tauri::App) {
    app.manage(LocalAttention::default());
    let handle = app.handle().clone();
    let spawned = thread::Builder::new()
        .name("openaide-local-attention".to_string())
        .spawn(move || {
            let mut watch = Watch::new();
            let mut post = post_loopback;
            loop {
                let target = watched_target(&handle);
                match watch.step(target, &mut post) {
                    Step::Idle => {
                        forget(&handle);
                        thread::sleep(IDLE_INTERVAL);
                    }
                    Step::Changed(tasks) => publish(&handle, tasks),
                    Step::Unchanged => {}
                    Step::Retry(delay) => thread::sleep(delay),
                }
            }
        });
    if spawned.is_err() {
        desktop_log!("desktop_local_attention_failed stage=start error_class=thread");
    }
}

/// This computer's App Server, but only while the window uses a paired computer's.
fn watched_target(app: &tauri::AppHandle) -> Option<HoldTarget> {
    let connections = app.state::<DesktopConnections>();
    let paired_in_use = matches!(connections.computer_in_use(), Some(Some(_)));
    paired_in_use
        .then(|| connections.local())
        .flatten()
        .map(|local| HoldTarget {
            endpoint_url: local.endpoint_url,
            auth_token: local.auth_token,
        })
}

fn publish(app: &tauri::AppHandle, tasks: Value) {
    if let Ok(mut latest) = app.state::<LocalAttention>().latest.lock() {
        *latest = Some(tasks.clone());
    }
    if app
        .emit(ATTENTION_EVENT, json!({ "tasks": tasks }))
        .is_err()
    {
        desktop_log!("desktop_local_attention_failed stage=emit error_class=window");
    }
}

fn forget(app: &tauri::AppHandle) {
    if let Ok(mut latest) = app.state::<LocalAttention>().latest.lock() {
        *latest = None;
    }
}

/// The latest list for a window that starts while a paired computer is in use;
/// `None` while nothing is watched yet.
#[tauri::command]
pub(crate) fn desktop_local_attention_snapshot(
    state: tauri::State<'_, LocalAttention>,
) -> Option<Value> {
    state.latest.lock().ok()?.clone()
}

#[cfg(test)]
#[path = "desktop_local_attention_tests.rs"]
mod tests;
