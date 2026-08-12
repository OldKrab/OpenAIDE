#[cfg(target_os = "windows")]
use std::io::{Read, Write};
#[cfg(target_os = "windows")]
use std::net::{TcpStream, ToSocketAddrs};
#[cfg(target_os = "windows")]
use std::os::windows::process::CommandExt;
use std::path::Path;
#[cfg(target_os = "windows")]
use std::process::{Command, Stdio};
#[cfg(target_os = "windows")]
use std::thread;
#[cfg(target_os = "windows")]
use std::time::Duration;

#[cfg(target_os = "windows")]
use crate::CREATE_NO_WINDOW;
#[cfg(target_os = "windows")]
use crate::desktop_runtime::decode_wsl_list;
use crate::{HandoffFailure, LocalHttpConnection, failure, read_handoff_line, validate_connection};

#[cfg(target_os = "windows")]
pub(crate) fn discover_wsl_distros() -> Vec<String> {
    let mut command = Command::new("wsl.exe");
    command.args(["--list", "--quiet"]);
    command.creation_flags(CREATE_NO_WINDOW);
    command
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| decode_wsl_list(&output.stdout))
        .unwrap_or_default()
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn discover_wsl_distros() -> Vec<String> {
    Vec::new()
}

#[cfg(target_os = "windows")]
pub(crate) fn translate_path_with_wsl(distro: &str, path: &str) -> std::io::Result<String> {
    let mut command = Command::new("wsl.exe");
    command.args(["--distribution", distro, "--exec", "wslpath", "-u", path]);
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output()?;
    if !output.status.success() {
        return Err(std::io::Error::other(
            "WSL could not translate the selected Windows folder.",
        ));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn translate_path_with_wsl(_distro: &str, _path: &str) -> std::io::Result<String> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "WSL path translation is available only on Windows.",
    ))
}

#[cfg(target_os = "windows")]
pub(crate) fn launch_wsl_app_server_handoff(
    distro: &str,
    resource_binary: &Path,
    progress: impl Fn(&'static str, String),
) -> Result<LocalHttpConnection, HandoffFailure> {
    progress("wsl_check", format!("Checking {distro}"));
    if !discover_wsl_distros()
        .iter()
        .any(|candidate| candidate.eq_ignore_ascii_case(distro))
    {
        return Err(failure(
            "wsl_distro_unavailable",
            "The selected WSL distribution is unavailable. Choose another environment in Settings.",
        ));
    }
    validate_linux_payload(resource_binary)?;
    progress(
        "wsl_install",
        format!("Installing OpenAIDE runtime in {distro}"),
    );
    let installed = install_runtime(distro, resource_binary)?;
    progress("wsl_launch", format!("Starting OpenAIDE in {distro}"));
    let launch_id = uuid::Uuid::new_v4().to_string();
    let mut child = launch_runtime(distro, &installed, &launch_id)?;
    let stdout = child.stdout.take().ok_or_else(|| {
        failure(
            "wsl_stdout_missing",
            "The WSL runtime did not provide startup information.",
        )
    })?;
    let connection = (|| {
        let line = read_handoff_line(stdout, &mut child)?;
        let connection: LocalHttpConnection = serde_json::from_str(line.trim()).map_err(|_| {
            failure(
                "wsl_json_invalid",
                "The WSL runtime returned invalid startup information.",
            )
        })?;
        validate_connection(&connection)?;
        progress("wsl_connect", format!("Connecting to OpenAIDE in {distro}"));
        probe_connection(&connection).map_err(|_| {
            failure(
                "wsl_localhost_unreachable",
                "WSL started, but its OpenAIDE server is unreachable from Windows. Check WSL localhost forwarding and try again.",
            )
        })?;
        Ok(connection)
    })();
    let connection = match connection {
        Ok(connection) => connection,
        Err(error) => {
            let _ = child.kill();
            terminate_failed_runtime(distro, &launch_id);
            return Err(error);
        }
    };
    thread::spawn(move || {
        let _ = child.wait();
    });
    Ok(connection)
}

#[cfg(not(target_os = "windows"))]
pub(crate) fn launch_wsl_app_server_handoff(
    _distro: &str,
    _resource_binary: &Path,
    _progress: impl Fn(&'static str, String),
) -> Result<LocalHttpConnection, HandoffFailure> {
    Err(failure(
        "wsl_platform_unsupported",
        "WSL is available only in OpenAIDE for Windows.",
    ))
}

#[cfg(target_os = "windows")]
fn validate_linux_payload(resource_binary: &Path) -> Result<(), HandoffFailure> {
    let mut magic = [0_u8; 4];
    std::fs::File::open(resource_binary)
        .and_then(|mut file| file.read_exact(&mut magic))
        .map_err(|_| {
            failure(
                "wsl_payload_missing",
                "OpenAIDE's bundled WSL runtime is missing.",
            )
        })?;
    if magic != [0x7f, b'E', b'L', b'F'] {
        return Err(failure(
            "wsl_payload_invalid",
            "OpenAIDE's bundled WSL runtime is invalid.",
        ));
    }
    Ok(())
}

#[cfg(target_os = "windows")]
fn install_runtime(distro: &str, resource_binary: &Path) -> Result<String, HandoffFailure> {
    let script = r#"set -eu
if [ "$(uname -m)" != "x86_64" ]; then exit 64; fi
source_path="$(wslpath -u "$1")"
install_root="$HOME/.local/share/openaide/app-server/$2"
target="$install_root/openaide-app-server"
mkdir -p "$install_root"
if [ ! -x "$target" ] || ! cmp -s "$source_path" "$target"; then
  temporary="$target.tmp.$$"
  cp "$source_path" "$temporary"
  chmod 700 "$temporary"
  mv -f "$temporary" "$target"
fi
printf '%s\n' "$target"
"#;
    let resource_path = resource_binary.to_string_lossy();
    let installed = run_wsl_output(
        distro,
        &[
            "/bin/sh",
            "-c",
            script,
            "openaide-install",
            resource_path.as_ref(),
            env!("CARGO_PKG_VERSION"),
        ],
    )
    .map_err(|error| {
        if error.kind() == std::io::ErrorKind::Unsupported {
            failure(
                "wsl_architecture_unsupported",
                "This OpenAIDE build supports x64 WSL distributions only.",
            )
        } else {
            failure(
                "wsl_install_failed",
                "OpenAIDE could not install its runtime in WSL.",
            )
        }
    })?;
    let installed = installed.trim().to_string();
    if installed.is_empty() {
        return Err(failure(
            "wsl_install_path_missing",
            "OpenAIDE could not verify its installed WSL runtime.",
        ));
    }
    Ok(installed)
}

#[cfg(target_os = "windows")]
fn launch_runtime(
    distro: &str,
    installed: &str,
    launch_id: &str,
) -> Result<std::process::Child, HandoffFailure> {
    let script = r#"set -eu
data_root="$HOME/.local/share/openaide"
mkdir -p "$data_root/state" "$data_root/runtime"
pid_file="$data_root/runtime/desktop-launch-$2.pid"
env OPENAIDE_APP_SERVER_PROTOCOL=app-server-handoff \
  OPENAIDE_STORAGE_ROOT="$data_root/state" \
  OPENAIDE_RUNTIME_ROOT="$data_root/runtime" "$1" &
server_pid="$!"
printf '%s\n' "$server_pid" > "$pid_file"
set +e
wait "$server_pid"
status="$?"
rm -f "$pid_file"
exit "$status"
"#;
    let mut command = Command::new("wsl.exe");
    command.args([
        "--distribution",
        distro,
        "--exec",
        "/bin/sh",
        "-c",
        script,
        "openaide-launch",
        installed,
        launch_id,
    ]);
    command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::inherit())
        .creation_flags(CREATE_NO_WINDOW)
        .spawn()
        .map_err(|_| failure("wsl_process_spawn", "OpenAIDE could not start WSL."))
}

#[cfg(target_os = "windows")]
fn run_wsl_output(distro: &str, args: &[&str]) -> std::io::Result<String> {
    let mut command = Command::new("wsl.exe");
    command
        .args(["--distribution", distro, "--exec"])
        .args(args);
    command.creation_flags(CREATE_NO_WINDOW);
    let output = command.output()?;
    if !output.status.success() {
        if output.status.code() == Some(64) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::Unsupported,
                "unsupported WSL architecture",
            ));
        }
        return Err(std::io::Error::other("WSL command failed"));
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

#[cfg(target_os = "windows")]
fn terminate_failed_runtime(distro: &str, launch_id: &str) {
    // The Windows wsl.exe wrapper can exit before its Linux child. The PID file
    // is written by the exec'ing shell, so it identifies only this Desktop launch.
    let script = r#"pid_file="$HOME/.local/share/openaide/runtime/desktop-launch-$1.pid"
if [ -f "$pid_file" ]; then
  pid="$(cat "$pid_file")"
  rm -f "$pid_file"
  kill "$pid" 2>/dev/null || true
fi
"#;
    let _ = run_wsl_output(
        distro,
        &["/bin/sh", "-c", script, "openaide-cleanup", launch_id],
    );
}

#[cfg(target_os = "windows")]
fn probe_connection(connection: &LocalHttpConnection) -> std::io::Result<()> {
    let endpoint = url::Url::parse(&connection.endpoint_url)
        .map_err(|_| std::io::Error::other("invalid endpoint"))?;
    let host = endpoint
        .host_str()
        .ok_or_else(|| std::io::Error::other("endpoint host missing"))?;
    let port = endpoint
        .port_or_known_default()
        .ok_or_else(|| std::io::Error::other("endpoint port missing"))?;
    let address = (host, port)
        .to_socket_addrs()?
        .find(|address| address.ip().is_loopback())
        .ok_or_else(|| std::io::Error::other("loopback address missing"))?;
    let mut stream = TcpStream::connect_timeout(&address, Duration::from_secs(5))?;
    stream.set_read_timeout(Some(Duration::from_secs(5)))?;
    stream.set_write_timeout(Some(Duration::from_secs(5)))?;
    let body =
        r#"{"jsonrpc":"2.0","id":"desktop-wsl-bootstrap","method":"client/probe","params":{}}"#;
    let path = if endpoint.path().is_empty() {
        "/probe"
    } else {
        endpoint.path()
    };
    write!(
        stream,
        "POST {path} HTTP/1.1\r\nHost: {host}:{port}\r\nAuthorization: Bearer {}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
        connection.auth_token,
        body.len(),
    )?;
    let mut response = String::new();
    stream.take(64 * 1024).read_to_string(&mut response)?;
    if !response.starts_with("HTTP/1.1 200") || !response.contains("\"result\"") {
        return Err(std::io::Error::other("authenticated WSL probe failed"));
    }
    Ok(())
}
