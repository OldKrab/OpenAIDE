use std::io::{self, BufRead, Write};
use std::sync::{Arc, Mutex};
use std::thread;

use openaide_app_server::client_lifecycle::AppServerTime;
use openaide_app_server::protocol_edge::stdio::ProtocolEdgeStdioDispatcher;
use openaide_app_server::task_events::{TaskUpdate, TaskUpdateReceiver};

#[cfg(test)]
#[path = "protocol_edge_process_tests.rs"]
mod tests;

pub(super) fn run_stdio(mut dispatcher: ProtocolEdgeStdioDispatcher) {
    let task_updates = dispatcher.take_task_updates();
    let worktree_updates = dispatcher.take_worktree_updates();
    let host_requests = dispatcher.take_host_requests();
    let dispatcher = Arc::new(Mutex::new(dispatcher));
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    if let Some(task_updates) = task_updates {
        let dispatcher = dispatcher.clone();
        let stdout = stdout.clone();
        thread::spawn(move || {
            for update in task_updates {
                let messages = dispatcher
                    .lock()
                    .expect("protocol edge dispatcher lock poisoned")
                    .handle_task_update(update);
                for message in messages {
                    let mut stdout = stdout.lock().expect("stdout lock poisoned");
                    if writeln!(stdout, "{message}").is_err() {
                        return;
                    }
                    let _ = stdout.flush();
                }
            }
        });
    }
    if let Some(worktree_updates) = worktree_updates {
        let dispatcher = dispatcher.clone();
        let stdout = stdout.clone();
        thread::spawn(move || {
            for repository in worktree_updates {
                let messages = dispatcher
                    .lock()
                    .expect("protocol edge dispatcher lock poisoned")
                    .handle_worktree_update(repository);
                for message in messages {
                    let mut stdout = stdout.lock().expect("stdout lock poisoned");
                    if writeln!(stdout, "{message}").is_err() {
                        return;
                    }
                    let _ = stdout.flush();
                }
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
                let mut stdout = stdout.lock().expect("stdout lock poisoned");
                if writeln!(stdout, "{line}").is_err() {
                    return;
                }
                let _ = stdout.flush();
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
        let responses = dispatcher
            .lock()
            .expect("protocol edge dispatcher lock poisoned")
            .handle_line(&line);
        for response in responses {
            let mut stdout = stdout.lock().expect("stdout lock poisoned");
            if writeln!(stdout, "{response}").is_err() {
                return;
            }
            let _ = stdout.flush();
        }
    }
    openaide_app_server::logging::info("protocol_edge_app_server_stopped", serde_json::json!({}));
}

pub(super) fn run_local_http_handoff(
    mut dispatcher: ProtocolEdgeStdioDispatcher,
    wait_for_last_client: impl FnOnce(),
) {
    debug_assert!(dispatcher.take_host_requests().is_none());
    if let Some(task_updates) = dispatcher.take_task_updates() {
        let gateway = dispatcher.shared_gateway();
        thread::spawn(move || {
            forward_local_http_task_updates(task_updates, |update| {
                gateway.publish_committed_task_update(&update, AppServerTime::now());
            });
        });
    }
    if let Some(worktree_updates) = dispatcher.take_worktree_updates() {
        let gateway = dispatcher.shared_gateway();
        thread::spawn(move || {
            for repository in worktree_updates {
                gateway.publish_worktree_repository_update(repository, AppServerTime::now());
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

fn forward_local_http_task_updates(
    task_updates: TaskUpdateReceiver,
    mut publish: impl FnMut(TaskUpdate),
) {
    for update in task_updates {
        publish(update);
    }
}
