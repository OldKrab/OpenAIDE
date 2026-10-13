use std::fs;
use std::process::Command;

#[test]
fn launch_imports_user_shell_environment_without_polluting_handoff_stdout() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("openaide-wsl-shell-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let shell = root.join("user-shell");
    let server = root.join("fake-app-server");
    fs::write(
        &shell,
        "#!/bin/sh\nprintf 'shell startup chatter\\n'\nshift\ncommand=$1\nshift\nexport PATH=\"/shell-managed/bin:$PATH\"\nexec /bin/sh -c \"$command\" \"$@\"\n",
    )
    .unwrap();
    fs::write(&server, "#!/bin/sh\nprintf 'PATH=%s\\n' \"$PATH\"\n").unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&server, fs::Permissions::from_mode(0o700)).unwrap();

    let output = Command::new("/bin/sh")
        .args([
            "-c",
            super::wsl_launch_script(),
            "openaide-launch-test",
            server.to_str().unwrap(),
            "launch-id",
            shell.to_str().unwrap(),
        ])
        .env("HOME", &root)
        .output()
        .unwrap();

    fs::remove_dir_all(&root).unwrap();
    assert!(output.status.success());
    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.starts_with("PATH=/shell-managed/bin:"), "{stdout}");
    assert!(!stdout.contains("shell startup chatter"), "{stdout}");
}

/// A startup file that prompts must fail instead of waiting on a terminal nobody
/// can see. A session leader has no controlling terminal until it opens one.
#[cfg(target_os = "linux")]
#[test]
fn launch_runs_user_shell_in_its_own_session() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("openaide-wsl-shell-test-{}", uuid::Uuid::new_v4()));
    fs::create_dir_all(&root).unwrap();
    let shell = root.join("user-shell");
    let server = root.join("fake-app-server");
    fs::write(
        &shell,
        "#!/bin/sh\nexport SHELL_SESSION=\"$(cut -d' ' -f6 /proc/$$/stat)\" SHELL_PID=\"$$\"\nshift\ncommand=$1\nshift\nexec /bin/sh -c \"$command\" \"$@\"\n",
    )
    .unwrap();
    fs::write(
        &server,
        "#!/bin/sh\n[ \"$SHELL_SESSION\" = \"$SHELL_PID\" ] && printf 'session-leader\\n'\n",
    )
    .unwrap();
    fs::set_permissions(&shell, fs::Permissions::from_mode(0o700)).unwrap();
    fs::set_permissions(&server, fs::Permissions::from_mode(0o700)).unwrap();

    let output = Command::new("/bin/sh")
        .args([
            "-c",
            super::wsl_launch_script(),
            "openaide-launch-test",
            server.to_str().unwrap(),
            "launch-id",
            shell.to_str().unwrap(),
        ])
        .env("HOME", &root)
        .output()
        .unwrap();

    fs::remove_dir_all(&root).unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap(),
        "session-leader\n"
    );
}

#[test]
fn launch_removes_pid_files_of_dead_launches_and_keeps_live_ones() {
    use std::os::unix::fs::PermissionsExt;

    let root =
        std::env::temp_dir().join(format!("openaide-wsl-shell-test-{}", uuid::Uuid::new_v4()));
    let runtime = root.join(".local/share/openaide/runtime");
    fs::create_dir_all(&runtime).unwrap();
    let dead = runtime.join("desktop-launch-dead.pid");
    let live = runtime.join("desktop-launch-live.pid");
    // Above every platform's PID range, so no process can own it.
    fs::write(&dead, "2000000000\n").unwrap();
    fs::write(&live, format!("{}\n", std::process::id())).unwrap();
    let server = root.join("fake-app-server");
    fs::write(&server, "#!/bin/sh\n").unwrap();
    fs::set_permissions(&server, fs::Permissions::from_mode(0o700)).unwrap();

    let status = Command::new("/bin/sh")
        .args([
            "-c",
            super::wsl_launch_script(),
            "openaide-launch-test",
            server.to_str().unwrap(),
            "launch-id",
            "/bin/sh",
        ])
        .env("HOME", &root)
        .status()
        .unwrap();

    let remaining = (dead.exists(), live.exists());
    fs::remove_dir_all(&root).unwrap();
    assert!(status.success());
    assert_eq!(remaining, (false, true));
}
