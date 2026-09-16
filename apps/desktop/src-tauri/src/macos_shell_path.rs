use std::ffi::OsString;
use std::fs;
use std::io::Read;
use std::os::unix::ffi::OsStringExt;
use std::os::unix::fs::DirBuilderExt;
use std::os::unix::process::CommandExt;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

/// Desktop owns terminal PATH discovery. Only the child receives it; no global
/// environment mutation races other Tauri threads or imports unrelated secrets.
pub(crate) fn apply_to(server: &mut Command) -> Result<(), &'static str> {
    let mut shell = Command::new(std::env::var_os("SHELL").unwrap_or_else(|| "/bin/zsh".into()));
    let started = Instant::now();
    eprintln!(
        "{{\"event\":\"desktop_shell_path_started\",\"operation\":\"desktop/shell_path\",\"attempt\":1}}"
    );
    let result = configure_from_shell(server, &mut shell, Duration::from_secs(10));
    eprintln!(
        "{{\"event\":\"desktop_shell_path_completed\",\"operation\":\"desktop/shell_path\",\"outcome\":\"{}\",\"duration_ms\":{}}}",
        result.as_ref().err().copied().unwrap_or("success"),
        started.elapsed().as_millis()
    );
    result
}

fn configure_from_shell(
    server: &mut Command,
    shell: &mut Command,
    timeout: Duration,
) -> Result<(), &'static str> {
    let scratch = PathCapture::new()?;
    let output = scratch.0.join("path");
    // The private file separates PATH from arbitrary startup banners. printenv
    // also works with shells such as fish whose variable syntax differs from sh.
    let child = shell
        .args([
            "-ilc",
            "/usr/bin/printenv PATH > \"$OPENAIDE_SHELL_PATH_FILE\"",
        ])
        .env("OPENAIDE_SHELL_PATH_FILE", &output)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()
        .map_err(|_| "shell_spawn")?;
    let mut probe = ShellProbe(child);
    let started = Instant::now();
    loop {
        if let Some(status) = probe.0.try_wait().map_err(|_| "shell_wait")? {
            if !status.success() {
                return Err("shell_exit");
            }
            break;
        }
        if started.elapsed() >= timeout {
            return Err("timeout");
        }
        std::thread::sleep(Duration::from_millis(10));
    }
    let mut path = Vec::new();
    fs::File::open(output)
        .map_err(|_| "path_missing")?
        .take(65537)
        .read_to_end(&mut path)
        .map_err(|_| "path_read")?;
    if path.last() == Some(&b'\n') {
        path.pop();
    }
    if path.is_empty() || path.len() > 65535 || path.contains(&0) {
        return Err("path_invalid");
    }
    server.env("PATH", OsString::from_vec(path));
    Ok(())
}

struct PathCapture(std::path::PathBuf);
impl PathCapture {
    fn new() -> Result<Self, &'static str> {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_err(|_| "clock")?
            .as_nanos();
        let path =
            std::env::temp_dir().join(format!("openaide-shell-{}-{unique}", std::process::id()));
        fs::DirBuilder::new()
            .mode(0o700)
            .create(&path)
            .map_err(|_| "capture_create")?;
        Ok(Self(path))
    }
}
impl Drop for PathCapture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

struct ShellProbe(Child);
impl Drop for ShellProbe {
    fn drop(&mut self) {
        if matches!(self.0.try_wait(), Ok(Some(_))) {
            return;
        }
        // Startup scripts can wait on children: stop the isolated process group,
        // then reap the shell. No inherited application process is in this group.
        let _ = Command::new("/bin/kill")
            .args(["-KILL", &format!("-{}", self.0.id())])
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .status();
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

#[cfg(test)]
#[path = "macos_shell_path_tests.rs"]
mod tests;
