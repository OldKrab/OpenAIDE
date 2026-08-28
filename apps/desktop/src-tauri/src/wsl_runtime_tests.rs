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
