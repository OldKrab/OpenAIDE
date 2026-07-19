use super::*;
use crate::client_lifecycle::AppServerTime;

#[test]
fn initialize_during_draining_aborts_draining() {
    let mut lifecycle = AppLifecycle::new();
    lifecycle.begin_draining();

    assert_eq!(
        lifecycle.admit_initialize(AppServerTime(1)),
        InitializeAdmission::AcceptedAndAbortedDraining
    );
    assert_eq!(lifecycle.state(), LifecycleState::Running);
}

#[test]
fn initialize_during_stopping_is_rejected() {
    let mut lifecycle = AppLifecycle::new();
    lifecycle.begin_stopping();

    assert!(matches!(
        lifecycle.admit_initialize(AppServerTime(1)),
        InitializeAdmission::Rejected(_)
    ));
}

#[test]
fn shutdown_request_returns_ordered_plan() {
    let mut lifecycle = AppLifecycle::new();

    let outcome = lifecycle.request_shutdown();

    assert_eq!(lifecycle.state(), LifecycleState::Stopping);
    assert_eq!(
        outcome,
        ShutdownRequestOutcome::ShutdownPlanned(ShutdownPlan {
            stop_accepting_new_work: true,
            interrupt_pending_requests: true,
            detach_agent_transports: true,
            remove_endpoint_records: true,
        })
    );
    assert_eq!(
        lifecycle.request_shutdown(),
        ShutdownRequestOutcome::AlreadyStopping
    );
}

#[test]
fn failed_shutdown_persistence_requires_unclean_lease_expiry() {
    let lifecycle = AppLifecycle::new();

    assert_eq!(
        lifecycle.complete_shutdown(false),
        ShutdownCompletion::UncleanLeaseExpiryRequired
    );
    assert_eq!(
        lifecycle.complete_shutdown(true),
        ShutdownCompletion::CleanRelease
    );
}
