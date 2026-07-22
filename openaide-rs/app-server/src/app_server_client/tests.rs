use super::*;
use crate::storage_runtime::TransportKind;
use crate::storage_runtime::{RuntimeEndpoint, RuntimeEndpointRecord, RuntimeEndpointRecordStatus};

#[test]
fn compatible_endpoint_is_reused() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(&request, &record, EndpointProbeOutcome::Compatible)),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::AttachExisting { target: target() }
    );
}

#[test]
fn stale_endpoint_is_cleaned_only_after_authoritative_probe_failure() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(&request, &record, EndpointProbeOutcome::Unreachable)),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::CleanStaleEndpoint {
            target: target(),
            reason: StaleEndpointReason::Unreachable,
        }
    );
}

#[test]
fn server_stopping_probe_cleans_stale_endpoint() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(
                &request,
                &record,
                EndpointProbeOutcome::ServerStopping
            )),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::CleanStaleEndpoint {
            target: target(),
            reason: StaleEndpointReason::ProbeReportedStopping,
        }
    );
}

#[test]
fn state_root_mismatch_probe_cleans_stale_endpoint() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(
                &request,
                &record,
                EndpointProbeOutcome::StateRootMismatch
            )),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::CleanStaleEndpoint {
            target: target(),
            reason: StaleEndpointReason::StateRootMismatch,
        }
    );
}

#[test]
fn endpoint_without_probe_requires_probe_before_cleanup_or_reuse() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            None,
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::ProbeEndpoint {
            target: target(),
            requirements: requirements(),
        }
    );
}

#[test]
fn stale_probe_report_requires_a_fresh_probe_for_current_endpoint() {
    let request = request("root-a");
    let current_record = record("root-a", RuntimeEndpointRecordStatus::Running);
    let stale_record = record("root-other", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&current_record),
            Some(&probe(
                &request,
                &stale_record,
                EndpointProbeOutcome::Compatible
            )),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::ProbeEndpoint {
            target: target(),
            requirements: requirements(),
        }
    );
}

#[test]
fn launch_lock_busy_reports_launch_in_progress() {
    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request("root-a"),
            None,
            None,
            LaunchLockState::Busy,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::WaitForLaunch
    );
}

#[test]
fn storage_writer_block_blocks_attach_or_launch() {
    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request("root-a"),
            None,
            None,
            LaunchLockState::Acquired,
            StorageWriterState::Blocked,
        ),
        AttachOrLaunchDecision::Fail {
            reason: AttachOrLaunchFailure::StorageStateRootBlocked,
        }
    );
}

#[test]
fn incompatible_protocol_endpoint_is_replaced() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(
                &request,
                &record,
                EndpointProbeOutcome::IncompatibleProtocol
            )),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::ReplaceIncompatible {
            target: target(),
            reason: AttachOrLaunchFailure::IncompatibleProtocol,
        }
    );
}

#[test]
fn incompatible_app_endpoint_is_replaced() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(
                &request,
                &record,
                EndpointProbeOutcome::IncompatibleApp
            )),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::ReplaceIncompatible {
            target: target(),
            reason: AttachOrLaunchFailure::IncompatibleApp,
        }
    );
}

#[test]
fn auth_failed_endpoint_is_not_reused_or_cleaned() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            Some(&probe(&request, &record, EndpointProbeOutcome::AuthFailed)),
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::Fail {
            reason: AttachOrLaunchFailure::AuthOrPermissionFailure,
        }
    );
}

#[test]
fn local_state_root_mismatch_cleans_without_probe() {
    let request = request("root-a");
    let record = record("root-b", RuntimeEndpointRecordStatus::Running);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            None,
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::CleanStaleEndpoint {
            target: EndpointTarget {
                state_root_fingerprint: "root-b".to_string(),
                ..target()
            },
            reason: StaleEndpointReason::StateRootMismatch,
        }
    );
}

#[test]
fn stopping_endpoint_hint_cleans_without_probe() {
    let request = request("root-a");
    let record = record("root-a", RuntimeEndpointRecordStatus::Stopping);

    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request,
            Some(&record),
            None,
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::CleanStaleEndpoint {
            target: target(),
            reason: StaleEndpointReason::EndpointRecordStopping,
        }
    );
}

#[test]
fn acquired_launch_lock_without_endpoint_launches_new_server() {
    assert_eq!(
        AttachOrLaunchDecider::decide(
            &request("root-a"),
            None,
            None,
            LaunchLockState::Acquired,
            StorageWriterState::Available,
        ),
        AttachOrLaunchDecision::LaunchNew
    );
}

fn request(fingerprint: &str) -> AttachOrLaunchRequest {
    AttachOrLaunchRequest {
        state_root_fingerprint: fingerprint.to_string(),
        required_protocol_version: "1".to_string(),
        required_app_version: "0.1.0".to_string(),
    }
}

fn record(fingerprint: &str, status: RuntimeEndpointRecordStatus) -> RuntimeEndpointRecord {
    RuntimeEndpointRecord {
        server_id: "server-1".to_string(),
        state_root_fingerprint: fingerprint.to_string(),
        pid: std::process::id(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        status,
        auth_token: "token".to_string(),
        replacement_token: Some("replacement-token".to_string()),
        endpoints: vec![RuntimeEndpoint {
            transport: TransportKind::LocalHttp,
            address: "http://127.0.0.1:12345".to_string(),
        }],
    }
}

fn requirements() -> EndpointRequirements {
    EndpointRequirements {
        required_protocol_version: "1".to_string(),
        required_app_version: "0.1.0".to_string(),
    }
}

fn target() -> EndpointTarget {
    EndpointTarget {
        server_id: "server-1".to_string(),
        state_root_fingerprint: "root-a".to_string(),
        protocol_version: "1".to_string(),
        app_version: "0.1.0".to_string(),
        auth_token: "token".to_string(),
        replacement_token: Some("replacement-token".to_string()),
        endpoints: vec![RuntimeEndpoint {
            transport: TransportKind::LocalHttp,
            address: "http://127.0.0.1:12345".to_string(),
        }],
    }
}

fn probe(
    request: &AttachOrLaunchRequest,
    record: &RuntimeEndpointRecord,
    outcome: EndpointProbeOutcome,
) -> EndpointProbeReport {
    EndpointProbeReport {
        target: EndpointTarget {
            server_id: record.server_id.clone(),
            state_root_fingerprint: record.state_root_fingerprint.clone(),
            protocol_version: record.protocol_version.clone(),
            app_version: record.app_version.clone(),
            auth_token: record.auth_token.clone(),
            replacement_token: record.replacement_token.clone(),
            endpoints: record.endpoints.clone(),
        },
        requirements: EndpointRequirements {
            required_protocol_version: request.required_protocol_version.clone(),
            required_app_version: request.required_app_version.clone(),
        },
        outcome,
    }
}
