use super::*;
use std::fs;
use std::os::unix::fs::{DirBuilderExt, PermissionsExt};
use std::path::PathBuf;
use std::time::{Instant, SystemTime, UNIX_EPOCH};

struct Fixture(PathBuf);
impl Fixture {
    fn new(startup: &str) -> Self {
        let root = std::env::temp_dir().join(format!(
            "openaide-shell-test-{}-{}",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap()
                .as_nanos()
        ));
        fs::DirBuilder::new().mode(0o700).create(&root).unwrap();
        fs::create_dir(root.join("bin")).unwrap();
        fs::write(
            root.join("bin/npm"),
            "#!/bin/sh\nprintf 'fixture-npm-ready\\n'\n",
        )
        .unwrap();
        fs::set_permissions(root.join("bin/npm"), fs::Permissions::from_mode(0o700)).unwrap();
        fs::write(root.join(".zshrc"), startup).unwrap();
        Self(root)
    }
    fn shell(&self) -> Command {
        let mut shell = Command::new("/bin/zsh");
        shell.env("ZDOTDIR", &self.0).env("PATH", "/usr/bin:/bin");
        shell
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

#[test]
fn desktop_child_can_launch_shell_installed_npm_without_startup_chatter() {
    let fixture = Fixture::new("printf 'startup chatter\\n'; export PATH=\"$ZDOTDIR/bin:$PATH\"\n");
    let mut server = Command::new("/bin/sh");
    server
        .env("PATH", "/usr/bin:/bin")
        .args(["-c", "exec npm --version"]);
    configure_from_shell(&mut server, &mut fixture.shell(), Duration::from_secs(5)).unwrap();
    let output = server.output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.stdout, b"fixture-npm-ready\n");
}

#[test]
fn blocked_shell_startup_has_a_bounded_failure() {
    let fixture = Fixture::new("/bin/sleep 30\n");
    let started = Instant::now();
    let result = configure_from_shell(
        &mut Command::new("/bin/sh"),
        &mut fixture.shell(),
        Duration::from_millis(100),
    );
    assert_eq!(result, Err("timeout"));
    assert!(started.elapsed() < Duration::from_secs(3));
}

#[test]
fn failing_shell_does_not_silently_keep_the_broken_path() {
    let fixture = Fixture::new("exit 1\n");
    assert_eq!(
        configure_from_shell(
            &mut Command::new("/bin/sh"),
            &mut fixture.shell(),
            Duration::from_secs(5)
        ),
        Err("shell_exit")
    );
}
