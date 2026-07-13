#[test]
fn app_server_protocol_mode_initializes_over_stdio() {
    let storage = TempDir::new().expect("storage root");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_openaide-app-server"))
        .env("OPENAIDE_STORAGE_ROOT", storage.path())
        .env("OPENAIDE_APP_SERVER_PROTOCOL", "app-server-protocol")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn protocol-edge runtime");

    let request = json!({
        "jsonrpc": "2.0",
        "id": "initialize",
        "method": "client/initialize",
        "params": {
            "clientInstanceId": "contract-client",
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {}
        }
    })
    .to_string();
    {
        let stdin = child.stdin.as_mut().expect("runtime stdin");
        use std::io::Write;
        writeln!(stdin, "{request}").expect("write initialize");
    }

    let line = wait_for_runtime_stdout_line(child.stdout.take().expect("runtime stdout"));
    let response: Value = serde_json::from_str(&line).expect("json response");
    assert_eq!(response["id"], "initialize");
    assert_eq!(
        response["result"]["result"]["snapshot"]["client"]["clientInstanceId"],
        "contract-client"
    );

    child.kill().expect("stop protocol-edge runtime");
    let _ = child.wait();
}

#[test]
fn default_app_server_mode_initializes_protocol_edge_over_stdio() {
    let storage = TempDir::new().expect("storage root");
    let mut child = std::process::Command::new(env!("CARGO_BIN_EXE_openaide-app-server"))
        .env("OPENAIDE_STORAGE_ROOT", storage.path())
        // The default-mode contract must not inherit the parent App Shell's
        // explicit handoff mode.
        .env_remove("OPENAIDE_APP_SERVER_PROTOCOL")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .expect("spawn default runtime");

    let request = json!({
        "jsonrpc": "2.0",
        "id": "initialize",
        "method": "client/initialize",
        "params": {
            "clientInstanceId": "default-client",
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {}
        }
    })
    .to_string();
    {
        let stdin = child.stdin.as_mut().expect("runtime stdin");
        use std::io::Write;
        writeln!(stdin, "{request}").expect("write initialize");
    }

    let line = wait_for_runtime_stdout_line(child.stdout.take().expect("runtime stdout"));
    let response: Value = serde_json::from_str(&line).expect("json response");
    assert_eq!(response["id"], "initialize");
    assert_eq!(
        response["result"]["result"]["snapshot"]["client"]["clientInstanceId"],
        "default-client"
    );

    child.kill().expect("stop default runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_mode_launches_and_serves_local_http() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));

    let response = post_local_http_initialize(
        connection["endpointUrl"].as_str().expect("endpoint url"),
        connection["authToken"].as_str().expect("auth token"),
        "contract-client",
    );

    assert_eq!(response[0]["id"], "initialize");
    assert_eq!(
        response[0]["result"]["result"]["snapshot"]["client"]["clientInstanceId"],
        "contract-client"
    );

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_stdout_is_bootstrap_only() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let stdout = child.stdout.take().expect("runtime stdout");
    let mut stdout = std::io::BufReader::new(stdout);
    let connection = read_handoff_connection(&mut stdout);

    let request = json!({
        "jsonrpc": "2.0",
        "id": "legacy-stdio-initialize",
        "method": "client/initialize",
        "params": {
            "clientInstanceId": "legacy-stdio-client",
            "shell": { "kind": "web" },
            "requestedSurface": { "kind": "home" },
            "capabilities": {}
        }
    });
    {
        use std::io::Write;
        writeln!(child.stdin.as_mut().expect("runtime stdin"), "{request}")
            .expect("write legacy stdio request");
    }

    let (tx, rx) = std::sync::mpsc::channel();
    let reader = std::thread::spawn(move || {
        let mut line = String::new();
        let result = std::io::BufRead::read_line(&mut stdout, &mut line);
        let _ = tx.send((result, line));
    });
    let early_output = rx.recv_timeout(Duration::from_millis(250)).ok();
    drop(child.stdin.take());
    let status = child.wait().expect("handoff exits after parent disconnect");
    let (read_result, line) = early_output.unwrap_or_else(|| {
        rx.recv_timeout(Duration::from_secs(2))
            .expect("handoff stdout closes after parent disconnect")
    });
    reader.join().expect("stdout reader exits");

    assert!(status.success());
    assert_eq!(read_result.expect("read handoff stdout"), 0);
    assert!(line.is_empty(), "unexpected post-handoff stdout: {line}");
    assert_eq!(connection["kind"], "localHttp");
}

#[test]
fn app_server_handoff_mode_reuses_existing_local_http_endpoint() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut server = spawn_handoff_runtime(&storage, &runtime);
    let first = handoff_connection(server.stdout.take().expect("server stdout"));

    let mut attach = spawn_handoff_runtime(&storage, &runtime);
    let second = handoff_connection(attach.stdout.take().expect("attach stdout"));
    let status = attach.wait().expect("attach process exits");

    assert!(status.success());
    assert_eq!(second["endpointUrl"], first["endpointUrl"]);
    assert_eq!(second["authToken"], first["authToken"]);

    server.kill().expect("stop handoff runtime");
    let _ = server.wait();
}

#[test]
fn app_server_handoff_user_can_create_new_task_and_send_first_prompt() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "new-task-client");
    let project_id = first_project_id(&initialize);

    let created = post_local_http_json(
        endpoint_url,
        auth_token,
        "new-task-client",
        json!({
            "jsonrpc": "2.0",
            "id": "create",
            "method": "task/create",
            "params": {
                "projectId": project_id,
                "agentId": "codex"
            }
        }),
    );
    let created_task = &created[0]["result"]["result"]["task"];
    let task_id = created_task["task"]["taskId"].as_str().expect("task id");
    assert_eq!(created_task["chat"]["items"].as_array().unwrap().len(), 0);
    let ready_task =
        wait_until_task_send_ready(endpoint_url, auth_token, "new-task-client", task_id);
    let revision = ready_task["revision"].as_u64().expect("task revision");

    let sent = send_task(
        endpoint_url,
        auth_token,
        "new-task-client",
        task_id,
        revision,
        "hello from web",
    );

    assert!(
        sent["turnId"].as_str().is_some(),
        "unexpected send response: {sent:#?}"
    );
    assert_eq!(sent["task"]["task"]["hasMessages"], true);

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_initializes_new_task_screen_with_renderable_project_and_agent_state() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "new-task-screen-client");
    let project_id = first_project_id(&initialize);

    let new_task_initialize = post_local_http_initialize_surface(
        endpoint_url,
        auth_token,
        "new-task-screen-client",
        json!({ "kind": "newTask", "projectId": project_id }),
    );
    let snapshot = &new_task_initialize[0]["result"]["result"]["snapshot"];

    assert_eq!(snapshot["client"]["surface"]["kind"], "newTask");
    assert_eq!(snapshot["client"]["surface"]["projectId"], project_id);
    assert!(snapshot["projects"]["projects"]
        .as_array()
        .expect("projects")
        .iter()
        .any(|project| project["projectId"] == project_id));
    assert!(snapshot["agents"]["agents"]
        .as_array()
        .expect("agents")
        .iter()
        .any(|agent| agent["agentId"] == "codex"));
    assert!(snapshot["activeTask"].is_null());

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_registers_vscode_workspace_project_without_task_history() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime_without_configured_projects(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");

    let initialized = post_local_http_json(
        endpoint_url,
        auth_token,
        "vscode-empty-workspace-client",
        json!({
            "jsonrpc": "2.0",
            "id": "initialize-vscode-workspace",
            "method": "client/initialize",
            "params": {
                "clientInstanceId": "vscode-empty-workspace-client",
                "shell": { "kind": "vscodeExtension" },
                "requestedSurface": {
                    "kind": "project",
                    "projectId": "project-d997698f027765f9"
                },
                "capabilities": {},
                "workspaceRoots": [{ "path": "/workspace/OpenAIDE" }]
            }
        }),
    );

    let snapshot = &initialized[0]["result"]["result"]["snapshot"];
    assert!(snapshot["projects"]["projects"]
        .as_array()
        .expect("project collection")
        .iter()
        .any(|project| {
            project["projectId"] == "project-d997698f027765f9"
                && project["label"] == "OpenAIDE"
        }));
    assert!(snapshot["tasks"]["tasks"]
        .as_array()
        .expect("task navigation")
        .is_empty());

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_user_can_reopen_prepared_new_task_after_reload_and_send() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "reload-client");
    let project_id = first_project_id(&initialize);

    let created = post_local_http_json(
        endpoint_url,
        auth_token,
        "reload-client",
        json!({
            "jsonrpc": "2.0",
            "id": "create",
            "method": "task/create",
            "params": {
                "projectId": project_id,
                "agentId": "codex"
            }
        }),
    );
    let task_id = created[0]["result"]["result"]["task"]["task"]["taskId"]
        .as_str()
        .expect("task id");

    let reinitialized = post_local_http_json(
        endpoint_url,
        auth_token,
        "reload-client",
        json!({
            "jsonrpc": "2.0",
            "id": "reinitialize",
            "method": "client/initialize",
            "params": {
                "clientInstanceId": "reload-client",
                "shell": { "kind": "web" },
                "requestedSurface": {
                    "kind": "task",
                    "taskId": task_id
                },
                "capabilities": {}
            }
        }),
    );
    assert_eq!(
        reinitialized[0]["result"]["result"]["snapshot"]["activeTask"]["task"]["taskId"],
        task_id
    );

    let opened = wait_until_task_send_ready(endpoint_url, auth_token, "reload-client", task_id);
    let revision = opened["revision"].as_u64().expect("task revision");

    let sent = send_task(
        endpoint_url,
        auth_token,
        "reload-client",
        task_id,
        revision,
        "after reload",
    );
    assert!(sent["turnId"].as_str().is_some());

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_task_created_in_one_tab_is_visible_to_another_tab() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "creator-tab");
    let project_id = first_project_id(&initialize);
    post_local_http_initialize(endpoint_url, auth_token, "reader-tab");

    let created = create_task(endpoint_url, auth_token, "creator-tab", &project_id);
    let task_id = created["task"]["taskId"].as_str().expect("task id");
    let ready_task = wait_until_task_send_ready(endpoint_url, auth_token, "creator-tab", task_id);
    let revision = ready_task["revision"].as_u64().expect("task revision");
    let sent = send_task(
        endpoint_url,
        auth_token,
        "creator-tab",
        task_id,
        revision,
        "visible from another tab",
    );
    assert_eq!(sent["task"]["task"]["hasMessages"], true);

    let opened_in_reader = open_task(endpoint_url, auth_token, "reader-tab", task_id);
    assert_eq!(opened_in_reader["task"]["taskId"], task_id);
    assert_eq!(opened_in_reader["task"]["hasMessages"], true);

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_reinitialize_home_lists_the_current_task_after_send() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "project-list-client");
    let project_id = first_project_id(&initialize);

    let created = create_task(endpoint_url, auth_token, "project-list-client", &project_id);
    let task_id = created["task"]["taskId"].as_str().expect("task id");
    let ready_task =
        wait_until_task_send_ready(endpoint_url, auth_token, "project-list-client", task_id);
    let revision = ready_task["revision"].as_u64().expect("task revision");
    send_task(
        endpoint_url,
        auth_token,
        "project-list-client",
        task_id,
        revision,
        "show this task in navigation",
    );

    let reinitialized = post_local_http_initialize(endpoint_url, auth_token, "project-list-client");
    let tasks = reinitialized[0]["result"]["result"]["snapshot"]["tasks"]["tasks"]
        .as_array()
        .expect("task navigation");
    let listed = tasks
        .iter()
        .find(|task| task["taskId"] == task_id)
        .expect("created task is listed after reload");
    assert_eq!(listed["projectId"], project_id);

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

#[test]
fn app_server_handoff_user_can_send_first_prompt_after_task_preparation() {
    let storage = TempDir::new().expect("storage root");
    let runtime = TempDir::new().expect("runtime root");
    let mut child = spawn_handoff_runtime(&storage, &runtime);
    let connection = handoff_connection(child.stdout.take().expect("runtime stdout"));
    let endpoint_url = connection["endpointUrl"].as_str().expect("endpoint url");
    let auth_token = connection["authToken"].as_str().expect("auth token");
    let initialize = post_local_http_initialize(endpoint_url, auth_token, "preparation-client");
    let project_id = first_project_id(&initialize);

    let created = post_local_http_json(
        endpoint_url,
        auth_token,
        "preparation-client",
        json!({
            "jsonrpc": "2.0",
            "id": "create",
            "method": "task/create",
            "params": {
                "projectId": project_id,
                "agentId": "codex"
            }
        }),
    );
    let task_id = created[0]["result"]["result"]["task"]["task"]["taskId"]
        .as_str()
        .expect("task id");
    assert_eq!(
        created[0]["result"]["result"]["task"]["preparation"]["kind"],
        "preparing"
    );
    assert_eq!(
        created[0]["result"]["result"]["task"]["sendCapability"]["state"],
        "loading"
    );
    let ready = wait_until_task_send_ready(endpoint_url, auth_token, "preparation-client", task_id);
    let ready_revision = ready["revision"].as_u64().expect("ready revision");

    let sent = send_task(
        endpoint_url,
        auth_token,
        "preparation-client",
        task_id,
        ready_revision,
        "first try",
    );
    assert!(sent["turnId"].as_str().is_some());

    child.kill().expect("stop handoff runtime");
    let _ = child.wait();
}

fn spawn_handoff_runtime(storage: &TempDir, runtime: &TempDir) -> std::process::Child {
    spawn_handoff_runtime_with_project_roots(storage, runtime, Some(storage.path()))
}

fn spawn_handoff_runtime_without_configured_projects(
    storage: &TempDir,
    runtime: &TempDir,
) -> std::process::Child {
    spawn_handoff_runtime_with_project_roots(storage, runtime, None)
}

fn spawn_handoff_runtime_with_project_roots(
    storage: &TempDir,
    runtime: &TempDir,
    project_roots: Option<&std::path::Path>,
) -> std::process::Child {
    let mut command = std::process::Command::new(env!("CARGO_BIN_EXE_openaide-app-server"));
    command
        .env("OPENAIDE_STORAGE_ROOT", storage.path())
        .env("OPENAIDE_RUNTIME_ROOT", runtime.path())
        .env("OPENAIDE_APP_SERVER_PROTOCOL", "app-server-handoff")
        .stdin(std::process::Stdio::piped())
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped());
    if let Some(project_roots) = project_roots {
        command.env("OPENAIDE_PROJECT_ROOTS", project_roots);
    } else {
        command.env_remove("OPENAIDE_PROJECT_ROOTS");
    }
    command.spawn()
        .expect("spawn handoff runtime")
}

fn handoff_connection(stdout: std::process::ChildStdout) -> Value {
    let mut stdout = std::io::BufReader::new(stdout);
    read_handoff_connection(&mut stdout)
}

fn read_handoff_connection(stdout: &mut impl std::io::BufRead) -> Value {
    let mut line = String::new();
    stdout
        .read_line(&mut line)
        .expect("read handoff connection line");
    let value: Value = serde_json::from_str(&line).expect("handoff json");
    assert_eq!(value["kind"], "localHttp");
    assert!(value["endpointUrl"].as_str().is_some());
    assert!(value["authToken"].as_str().is_some());
    value
}

fn post_local_http_initialize(endpoint_url: &str, auth_token: &str, client_id: &str) -> Value {
    post_local_http_initialize_surface(
        endpoint_url,
        auth_token,
        client_id,
        json!({ "kind": "home" }),
    )
}

fn post_local_http_initialize_surface(
    endpoint_url: &str,
    auth_token: &str,
    client_id: &str,
    requested_surface: Value,
) -> Value {
    post_local_http_json(
        endpoint_url,
        auth_token,
        client_id,
        json!({
        "jsonrpc": "2.0",
        "id": "initialize",
        "method": "client/initialize",
        "params": {
            "clientInstanceId": client_id,
            "shell": { "kind": "web" },
            "requestedSurface": requested_surface,
            "capabilities": {}
        }
        }),
    )
}

fn post_local_http_json(
    endpoint_url: &str,
    auth_token: &str,
    connection_id: &str,
    request: Value,
) -> Value {
    let request = request.to_string();
    let response = local_http_post(endpoint_url, auth_token, connection_id, &request);
    serde_json::from_str(&response).expect("local http json response")
}

fn open_task(endpoint_url: &str, auth_token: &str, connection_id: &str, task_id: &str) -> Value {
    let opened = post_local_http_json(
        endpoint_url,
        auth_token,
        connection_id,
        json!({
            "jsonrpc": "2.0",
            "id": "open",
            "method": "task/open",
            "params": { "taskId": task_id }
        }),
    );
    opened[0]["result"]["result"]["task"].clone()
}

fn create_task(
    endpoint_url: &str,
    auth_token: &str,
    connection_id: &str,
    project_id: &str,
) -> Value {
    let created = post_local_http_json(
        endpoint_url,
        auth_token,
        connection_id,
        json!({
            "jsonrpc": "2.0",
            "id": "create",
            "method": "task/create",
            "params": {
                "projectId": project_id,
                "agentId": "codex"
            }
        }),
    );
    created[0]["result"]["result"]["task"].clone()
}

fn send_task(
    endpoint_url: &str,
    auth_token: &str,
    connection_id: &str,
    task_id: &str,
    _revision: u64,
    text: &str,
) -> Value {
    let response = post_local_http_json(
        endpoint_url,
        auth_token,
        connection_id,
        json!({
            "jsonrpc": "2.0",
            "id": "send",
            "method": "task/send",
            "params": {
                "taskId": task_id,
                "message": { "text": text }
            }
        }),
    );
    assert!(
        response[0]["result"]["result"].is_object(),
        "task/send failed: {response:#?}"
    );
    response[0]["result"]["result"].clone()
}

fn wait_until_task_send_ready(
    endpoint_url: &str,
    auth_token: &str,
    connection_id: &str,
    task_id: &str,
) -> Value {
    let deadline = std::time::Instant::now() + Duration::from_secs(10);
    loop {
        let task = open_task(endpoint_url, auth_token, connection_id, task_id);
        if task["sendCapability"]["state"] == "ready" {
            return task;
        }
        assert!(
            std::time::Instant::now() < deadline,
            "timed out waiting for task send readiness: {task}"
        );
        std::thread::sleep(Duration::from_millis(10));
    }
}

fn first_project_id(initialize_response: &Value) -> String {
    initialize_response[0]["result"]["result"]["snapshot"]["projects"]["projects"][0]["projectId"]
        .as_str()
        .expect("first project id")
        .to_string()
}

fn local_http_post(
    endpoint_url: &str,
    auth_token: &str,
    connection_id: &str,
    body: &str,
) -> String {
    let endpoint = endpoint_url
        .strip_prefix("http://")
        .expect("loopback http endpoint");
    let (host_port, path) = endpoint
        .split_once('/')
        .map(|(host_port, path)| (host_port, format!("/{path}")))
        .expect("endpoint path");
    let mut stream = std::net::TcpStream::connect(host_port).expect("connect local http");
    use std::io::{Read, Write};
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host_port}\r\nAuthorization: Bearer {auth_token}\r\nX-OpenAIDE-Connection-Id: {connection_id}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n\r\n{body}",
        body.len()
    )
    .expect("write local http request");
    let mut response = String::new();
    stream
        .read_to_string(&mut response)
        .expect("read local http response");
    response
        .split_once("\r\n\r\n")
        .map(|(_, body)| body.to_string())
        .expect("http response body")
}

fn wait_for_runtime_stdout_line(stdout: std::process::ChildStdout) -> String {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let mut lines = std::io::BufRead::lines(std::io::BufReader::new(stdout));
        let _ = tx.send(lines.next());
    });
    rx.recv_timeout(Duration::from_secs(2))
        .expect("runtime did not write a response")
        .expect("runtime stdout closed before response")
        .expect("runtime stdout line")
}
