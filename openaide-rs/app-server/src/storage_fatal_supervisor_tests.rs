use super::spawn_storage_fatal_supervisor;
use openaide_app_server::storage::task_journal::TaskStorageFatalFailure;

#[test]
fn root_fatal_signal_invokes_process_termination_owner_once() {
    let (failure_sender, failures) = std::sync::mpsc::channel();
    let (terminated_sender, terminated) = std::sync::mpsc::channel();
    let supervisor = spawn_storage_fatal_supervisor(failures, move |failure| {
        terminated_sender
            .send(failure.reason)
            .expect("report termination callback");
    });

    failure_sender
        .send(TaskStorageFatalFailure {
            reason: "worker_panicked",
        })
        .expect("send root fatal signal");
    assert_eq!(
        terminated
            .recv_timeout(std::time::Duration::from_secs(1))
            .expect("termination owner receives signal"),
        "worker_panicked"
    );
    supervisor.join().expect("join test supervisor");
}
