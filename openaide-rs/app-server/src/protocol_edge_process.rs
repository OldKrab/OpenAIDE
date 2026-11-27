use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

use openaide_app_server::protocol_edge::run_task_update_publisher;
use openaide_app_server::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;

/// Bounds how long a queued server request waits when nothing signals a delivery.
const STDIO_FLUSH_FALLBACK_WAKE: Duration = Duration::from_millis(250);

/// Every line App Server sends to stdout, except host bridge requests, leaves through the stdio
/// connection's outbound sequence. Publisher threads only publish; the flusher moves the sequence
/// to stdout on each delivery signal, and the stdin loop flushes after each line so replies and
/// rejections do not wait for a wake.
pub(super) fn run_stdio(mut dispatcher: ProtocolEdgeStdioDispatcher) {
    let task_updates = dispatcher.take_task_updates();
    let worktree_updates = dispatcher.take_worktree_updates();
    let agent_status_updates = dispatcher.take_agent_status_updates();
    let host_requests = dispatcher.take_host_requests();
    let outbound = dispatcher.outbound();
    let dispatcher = Arc::new(Mutex::new(dispatcher));
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    if let Some(task_updates) = task_updates {
        let dispatcher = dispatcher.clone();
        thread::spawn(move || {
            for update in task_updates {
                lock(&dispatcher).publish_task_update(update);
            }
        });
    }
    if let Some(worktree_updates) = worktree_updates {
        let dispatcher = dispatcher.clone();
        thread::spawn(move || {
            for repository in worktree_updates {
                lock(&dispatcher).publish_worktree_update(repository);
            }
        });
    }
    if let Some(agent_status_updates) = agent_status_updates {
        let dispatcher = dispatcher.clone();
        thread::spawn(move || {
            for _ in agent_status_updates {
                lock(&dispatcher).publish_agent_status_update();
            }
        });
    }
    {
        let outbound = outbound.clone();
        let stdout = stdout.clone();
        thread::spawn(move || {
            let signal = outbound.delivery_signal();
            loop {
                let seen = signal.generation();
                if !outbound.flush_with_server_requests(|lines| write_lines(&stdout, lines)) {
                    return;
                }
                signal.wait_changed(seen, STDIO_FLUSH_FALLBACK_WAKE);
            }
        });
    }
    if let Some(host_requests) = host_requests {
        let stdout = stdout.clone();
        thread::spawn(move || {
            for request in host_requests {
                let Ok(line) = serde_json::to_string(&request) else {
                    continue;
                };
                if !write_lines(&stdout, vec![line]) {
                    return;
                }
            }
        });
    }
    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }
        lock(&dispatcher).dispatch_line(&line);
        if !outbound.flush(|lines| write_lines(&stdout, lines)) {
            return;
        }
    }
    openaide_app_server::logging::info("protocol_edge_app_server_stopped", serde_json::json!({}));
}

fn lock(
    dispatcher: &Mutex<ProtocolEdgeStdioDispatcher>,
) -> std::sync::MutexGuard<'_, ProtocolEdgeStdioDispatcher> {
    dispatcher
        .lock()
        .expect("protocol edge dispatcher lock poisoned")
}

/// Writes lines to stdout. False once stdout is closed, which ends the writer.
fn write_lines(stdout: &Mutex<io::Stdout>, lines: Vec<String>) -> bool {
    if lines.is_empty() {
        return true;
    }
    let mut stdout = stdout.lock().expect("stdout lock poisoned");
    for line in lines {
        if writeln!(stdout, "{line}").is_err() {
            return false;
        }
    }
    let _ = stdout.flush();
    true
}

pub(super) fn run_local_http_handoff(
    mut dispatcher: ProtocolEdgeStdioDispatcher,
    wait_for_last_client: impl FnOnce(),
) {
    // Names stay distinct within the 15-character Linux thread name, so profiler
    // thread samples identify which publisher holds the protocol lock.
    if let Some(host_requests) = dispatcher.take_host_requests() {
        let gateway = dispatcher.shared_gateway();
        let host_bridge = dispatcher.host_bridge();
        spawn_named("openaide-host-requests", move || {
            for request in host_requests {
                let response = gateway.respond_to_acp_host_request(&request);
                let _ = host_bridge.try_handle_response(&response);
            }
        });
    }
    if let Some(task_updates) = dispatcher.take_task_updates() {
        let gateway = dispatcher.shared_gateway();
        spawn_named("openaide-pub-tasks", move || {
            run_task_update_publisher(task_updates, gateway);
        });
    }
    if let Some(worktree_updates) = dispatcher.take_worktree_updates() {
        let gateway = dispatcher.shared_gateway();
        spawn_named("openaide-pub-worktrees", move || {
            for repository in worktree_updates {
                gateway.publish_worktree_repository_update(repository);
            }
        });
    }
    if let Some(agent_status_updates) = dispatcher.take_agent_status_updates() {
        let gateway = dispatcher.shared_gateway();
        spawn_named("openaide-pub-agents", move || {
            for _ in agent_status_updates {
                gateway.publish_agent_status_update();
            }
        });
    }

    // The state-root App Server outlives whichever App Shell happened to launch it.
    wait_for_last_client();
    openaide_app_server::logging::info(
        "protocol_edge_local_http_handoff_stopped",
        serde_json::json!({}),
    );
}

/// A missing publisher would silently stop client updates, so spawn failure stays fatal.
fn spawn_named(name: &str, work: impl FnOnce() + Send + 'static) {
    if let Err(error) = thread::Builder::new().name(name.to_string()).spawn(work) {
        openaide_app_server::logging::error(
            "protocol_edge_thread_spawn_failed",
            serde_json::json!({ "operation": name, "error_kind": format!("{:?}", error.kind()) }),
        );
        panic!("failed to spawn {name}: {error}");
    }
}
