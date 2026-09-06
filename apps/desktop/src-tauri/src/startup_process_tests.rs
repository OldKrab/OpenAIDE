use super::StartupChild;

#[cfg(unix)]
use std::io::{BufRead, BufReader};

#[cfg(unix)]
use std::process::{Child, Command, Stdio};

#[cfg(unix)]
fn waiting_child() -> Child {
    Command::new("/bin/sh")
        .args(["-c", "printf 'startup-fixture\\n'; exec sleep 60"])
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .expect("start fixture process")
}

#[cfg(unix)]
#[test]
fn rejected_startup_stops_and_reaps_the_process() {
    let mut child = waiting_child();
    let rejected = (|| {
        let mut startup = StartupChild::new(&mut child);
        let stdout = startup.take_stdout().expect("startup stdout");
        let mut line = String::new();
        BufReader::new(stdout)
            .read_line(&mut line)
            .expect("fixture ready");
        assert_eq!(line, "startup-fixture\n");
        // JSON parsing, endpoint validation, and reachability checks all return
        // through this same guard before successful handoff relinquishes ownership.
        Err::<(), _>("invalid startup information")
    })();
    assert!(rejected.is_err());
    #[cfg(target_os = "linux")]
    let process_still_exists = std::path::Path::new(&format!("/proc/{}", child.id())).exists();
    let status = child.try_wait().expect("inspect rejected process");
    if status.is_none() {
        child.kill().expect("clean up regression fixture");
        child.wait().expect("reap regression fixture");
    }
    assert!(
        status.is_some(),
        "a rejected handoff must stop and reap its process before returning"
    );
    #[cfg(target_os = "linux")]
    assert!(
        !process_still_exists,
        "cleanup must reap the child before the caller inspects its status"
    );
}

#[cfg(unix)]
#[test]
fn accepted_startup_keeps_the_process_alive_for_its_supervisor() {
    let mut child = waiting_child();
    let startup = StartupChild::new(&mut child);
    startup.accept();
    let status = child.try_wait().expect("inspect accepted process");
    child.kill().expect("stop accepted fixture");
    child.wait().expect("reap accepted fixture");
    assert!(
        status.is_none(),
        "an accepted handoff must preserve the App Server process"
    );
}
