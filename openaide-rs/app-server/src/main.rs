use std::env;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::thread;

use openaide_app_server::protocol_edge::stdio::{
    AcpHostRequestTransport, ProtocolEdgeStdioDispatcher,
};
use openaide_app_server::storage_runtime::StateRoot;
use openaide_app_server::transport::shell_control::ShellControlDispatcher;
use openaide_app_server::Runtime;

mod app_server_handoff;
mod protocol_edge_process;

const APP_SERVER_PROTOCOL_MODE_ENV: &str = "OPENAIDE_APP_SERVER_PROTOCOL";
const LEGACY_PROTOCOL_MODE_ENV: &str = "OPENAIDE_RUNTIME_PROTOCOL";
const APP_SERVER_PROTOCOL_MODE: &str = "app-server-protocol";
const APP_SERVER_HANDOFF_MODE: &str = "app-server-handoff";
const SHELL_CONTROL_STDIO_MODE: &str = "shell-control-stdio";
const RUNTIME_ROOT_ENV: &str = "OPENAIDE_RUNTIME_ROOT";

fn main() {
    let storage_root = env::var_os("OPENAIDE_STORAGE_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(default_storage_root);
    openaide_app_server::logging::init_file_logger(&storage_root);
    openaide_app_server::logging::info("app_server_starting", serde_json::json!({}));

    match protocol_mode().as_deref() {
        Ok(APP_SERVER_PROTOCOL_MODE) => {
            run_protocol_edge_stdio(storage_root, ProtocolEdgeStartup::Plain);
            return;
        }
        Ok(APP_SERVER_HANDOFF_MODE) => {
            run_protocol_edge_stdio(storage_root, ProtocolEdgeStartup::LocalHttpHandoff);
            return;
        }
        Ok(SHELL_CONTROL_STDIO_MODE) => {
            run_shell_control_stdio(storage_root);
            return;
        }
        _ => {}
    }

    run_protocol_edge_stdio(storage_root, ProtocolEdgeStartup::Plain);
}

fn run_shell_control_stdio(storage_root: PathBuf) {
    let (runtime, task_updates, host_requests) = match Runtime::new_with_events(storage_root) {
        Ok(runtime) => runtime,
        Err(error) => {
            openaide_app_server::logging::error(
                "app_server_start_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
            eprintln!("failed to start OpenAIDE App Server: {error}");
            std::process::exit(1);
        }
    };
    start_storage_fatal_supervisor(runtime.take_storage_fatal_events());

    let mut dispatcher = ShellControlDispatcher::new(runtime);
    let stdin = io::stdin();
    let stdout = Arc::new(Mutex::new(io::stdout()));
    let task_update_stdout = stdout.clone();
    thread::spawn(move || {
        for update in task_updates {
            let notification =
                openaide_app_server::protocol::notifications::RuntimeNotification::task_updated(
                    &update.task_id,
                    update.revision,
                );
            let Ok(line) = serde_json::to_string(&notification) else {
                continue;
            };
            let mut stdout = task_update_stdout.lock().expect("stdout lock poisoned");
            if writeln!(stdout, "{line}").is_err() {
                return;
            }
            let _ = stdout.flush();
        }
    });
    let host_stdout = stdout.clone();
    thread::spawn(move || {
        for request in host_requests {
            let Ok(line) = serde_json::to_string(&request) else {
                continue;
            };
            let mut stdout = host_stdout.lock().expect("stdout lock poisoned");
            if writeln!(stdout, "{line}").is_err() {
                return;
            }
            let _ = stdout.flush();
        }
    });

    for line in stdin.lock().lines() {
        let Ok(line) = line else {
            break;
        };
        if line.trim().is_empty() {
            continue;
        }

        for response in dispatcher.handle_line(&line) {
            let mut stdout = stdout.lock().expect("stdout lock poisoned");
            if writeln!(stdout, "{response}").is_err() {
                return;
            }
            let _ = stdout.flush();
        }

        if dispatcher.shutdown_requested() {
            openaide_app_server::logging::info(
                "app_server_shutdown_requested",
                serde_json::json!({}),
            );
            break;
        }
    }
    openaide_app_server::logging::info("app_server_stopped", serde_json::json!({}));
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ProtocolEdgeStartup {
    Plain,
    LocalHttpHandoff,
}

fn run_protocol_edge_stdio(storage_root: PathBuf, startup: ProtocolEdgeStartup) {
    let runtime_root = env::var_os(RUNTIME_ROOT_ENV)
        .map(PathBuf::from)
        .unwrap_or_else(default_runtime_root);
    let state_root = match StateRoot::resolve(&storage_root) {
        Ok(state_root) => state_root,
        Err(error) => {
            openaide_app_server::logging::error(
                "protocol_edge_state_root_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
            eprintln!("failed to start OpenAIDE App Server Protocol: {error}");
            std::process::exit(1);
        }
    };
    let launch_lock = match startup {
        ProtocolEdgeStartup::Plain => None,
        ProtocolEdgeStartup::LocalHttpHandoff => {
            match app_server_handoff::attach_or_launch(&state_root, &runtime_root) {
                app_server_handoff::HandoffStart::Attached(connection) => {
                    app_server_handoff::print_connection_or_exit(&connection);
                    return;
                }
                app_server_handoff::HandoffStart::LaunchHere(lock) => Some(lock),
            }
        }
    };
    let host_request_transport = match startup {
        ProtocolEdgeStartup::Plain => AcpHostRequestTransport::Stdio,
        ProtocolEdgeStartup::LocalHttpHandoff => AcpHostRequestTransport::Unavailable,
    };
    let mut dispatcher = match ProtocolEdgeStdioDispatcher::try_new_with_host_request_transport(
        state_root.clone(),
        host_request_transport,
    ) {
        Ok(dispatcher) => dispatcher,
        Err(error) => {
            openaide_app_server::logging::error(
                "protocol_edge_store_open_failed",
                serde_json::json!({ "error": error.to_string() }),
            );
            eprintln!("failed to open OpenAIDE App Server Protocol storage: {error}");
            std::process::exit(1);
        }
    };
    start_storage_fatal_supervisor(
        dispatcher
            .take_storage_fatal_events()
            .expect("protocol edge owns Task storage fatal stream"),
    );
    let published_endpoint =
        match openaide_app_server::app_server_process::publish_local_http_probe_endpoint(
            dispatcher.shared_gateway(),
            &state_root,
            &runtime_root,
        ) {
            Ok(endpoint) => endpoint,
            Err(error) => {
                openaide_app_server::logging::error(
                    "protocol_edge_endpoint_publish_failed",
                    serde_json::json!({ "error": error.to_string() }),
                );
                eprintln!("failed to publish OpenAIDE App Server endpoint: {error}");
                std::process::exit(1);
            }
        };
    if startup == ProtocolEdgeStartup::LocalHttpHandoff {
        app_server_handoff::print_connection_or_exit(
            &published_endpoint
                .local_http_connection()
                .expect("published endpoint includes LocalHttp connection")
                .into(),
        );
    }
    let _launch_lock = launch_lock;
    match startup {
        ProtocolEdgeStartup::Plain => protocol_edge_process::run_stdio(dispatcher),
        ProtocolEdgeStartup::LocalHttpHandoff => {
            protocol_edge_process::run_local_http_handoff(dispatcher, || {
                if let Err(error) = published_endpoint.shutdown_after_last_client() {
                    openaide_app_server::logging::error(
                        "local_http_last_client_shutdown_failed",
                        serde_json::json!({ "error": error.to_string() }),
                    );
                }
            })
        }
    }
}

/// The binary owns the root-wide storage failure signal. A dead sole writer
/// invalidates durability for the entire state-root epoch, so graceful request
/// handling is no longer safe and the process must terminate non-zero.
fn start_storage_fatal_supervisor(
    failures: std::sync::mpsc::Receiver<
        openaide_app_server::storage::task_journal::TaskStorageFatalFailure,
    >,
) {
    spawn_storage_fatal_supervisor(failures, |_| std::process::exit(1));
}

fn spawn_storage_fatal_supervisor(
    failures: std::sync::mpsc::Receiver<
        openaide_app_server::storage::task_journal::TaskStorageFatalFailure,
    >,
    terminate: impl FnOnce(openaide_app_server::storage::task_journal::TaskStorageFatalFailure)
        + Send
        + 'static,
) -> thread::JoinHandle<()> {
    thread::Builder::new()
        .name("openaide-storage-fatal-supervisor".to_string())
        .spawn(move || {
            let Ok(failure) = failures.recv() else {
                return;
            };
            openaide_app_server::logging::error(
                "app_server_storage_fatal",
                serde_json::json!({ "reason": failure.reason }),
            );
            terminate(failure);
        })
        .expect("Task storage fatal supervisor must start")
}

fn protocol_mode() -> Result<String, env::VarError> {
    env::var(APP_SERVER_PROTOCOL_MODE_ENV).or_else(|_| env::var(LEGACY_PROTOCOL_MODE_ENV))
}

fn default_storage_root() -> PathBuf {
    env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".local/share")))
        .unwrap_or_else(|| PathBuf::from("."))
        .join("openaide")
}

fn default_runtime_root() -> PathBuf {
    env::var_os("XDG_RUNTIME_DIR")
        .map(PathBuf::from)
        .or_else(|| env::var_os("XDG_CACHE_HOME").map(PathBuf::from))
        .or_else(|| env::var_os("HOME").map(|home| PathBuf::from(home).join(".cache")))
        .unwrap_or_else(env::temp_dir)
        .join("openaide")
        .join("runtime")
}

#[cfg(test)]
#[path = "storage_fatal_supervisor_tests.rs"]
mod storage_fatal_supervisor_tests;
