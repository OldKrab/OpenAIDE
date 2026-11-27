use std::collections::HashMap;
use std::fs;

use tempfile::Builder;

use super::{artifact_path, load as load_since, prepare, prepare_with_faults, reconcile};
use crate::protocol::model::ActivityToolDetails;
use crate::storage::task_journal::frame::{FaultInjector, FaultPoint, JournalKind};
use crate::storage::task_journal::model::ArtifactOperation;
use crate::storage::task_journal::store::RecoveredTask;

/// The artifact as committed through `head`, without a known sequence.
fn load(
    root: &std::path::Path,
    task_id: &str,
    artifact_id: &str,
    head: u64,
) -> Result<
    crate::storage::task_journal::ToolArtifactProjection,
    crate::protocol::errors::RuntimeError,
> {
    load_since(root, task_id, artifact_id, head, head).map(|(artifact, _)| artifact)
}

const SENTINEL_ROOT: &str = "SENSITIVE_ABSOLUTE_ARTIFACT_ROOT";

#[test]
fn artifact_load_errors_never_disclose_the_state_root() {
    let root = Builder::new()
        .prefix(SENTINEL_ROOT)
        .tempdir()
        .expect("create sentinel state root");

    let message = load(root.path(), "task_1", "artifact_1", 1)
        .expect_err("missing artifact must fail")
        .to_string();

    assert!(message.contains("replay_open"));
    assert!(message.contains("not_found"));
    assert!(!message.contains(SENTINEL_ROOT));
    assert!(!message.contains(&root.path().display().to_string()));
}

#[test]
fn committed_artifact_append_does_not_replay_or_truncate_history() {
    let root = Builder::new()
        .prefix("artifact-orphan-tail")
        .tempdir()
        .expect("create state root");

    prepare(
        root.path(),
        "task_1",
        "artifact_1",
        0,
        vec![ArtifactOperation::AppendTerminal {
            terminal_id: "terminal_1".to_string(),
            data: "committed".to_string(),
        }],
    )
    .expect("prepare first committed frame");
    let faults = FaultInjector::armed(JournalKind::Artifact, FaultPoint::TruncateOpen);
    prepare_with_faults(
        root.path(),
        "task_1",
        "artifact_1",
        1,
        vec![ArtifactOperation::AppendTerminal {
            terminal_id: "terminal_1".to_string(),
            data: "-next".to_string(),
        }],
        &faults,
    )
    .expect("append without replaying or truncating history");

    assert!(
        faults.pending(),
        "normal append unexpectedly attempted truncation"
    );
    let artifact = load(root.path(), "task_1", "artifact_1", 2).expect("load committed frames");
    assert_eq!(artifact.terminal_outputs["terminal_1"], "committed-next");
}

#[test]
fn replay_restores_latest_details_and_first_seen_terminal_order() {
    let root = Builder::new()
        .prefix("artifact-projection")
        .tempdir()
        .expect("create state root");
    let details = ActivityToolDetails {
        locations: Vec::new(),
        content: Vec::new(),
        input: None,
        output: None,
    };

    let change = prepare(
        root.path(),
        "task_1",
        "artifact_1",
        0,
        vec![
            ArtifactOperation::ReplaceDetails {
                details: Box::new(details),
            },
            ArtifactOperation::AppendTerminal {
                terminal_id: "terminal_b".to_string(),
                data: "first".to_string(),
            },
            ArtifactOperation::AppendTerminal {
                terminal_id: "terminal_a".to_string(),
                data: "second".to_string(),
            },
            ArtifactOperation::AppendTerminal {
                terminal_id: "terminal_b".to_string(),
                data: "-third".to_string(),
            },
        ],
    )
    .expect("prepare artifact projection");

    assert_eq!(change.terminal_appends.len(), 3);
    let artifact = load(root.path(), "task_1", "artifact_1", 1).expect("load artifact projection");
    assert!(artifact.details.is_some());
    assert_eq!(
        artifact.terminal_order,
        vec!["terminal_b".to_string(), "terminal_a".to_string()]
    );
    assert_eq!(artifact.terminal_outputs["terminal_b"], "first-third");
    assert_eq!(artifact.terminal_outputs["terminal_a"], "second");
}

#[test]
fn artifact_append_faults_never_expose_bytes_above_the_task_head() {
    for point in [
        FaultPoint::AppendOpen,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
    ] {
        let root = Builder::new()
            .prefix("artifact-append-fault")
            .tempdir()
            .expect("create state root");
        prepare(
            root.path(),
            "task_1",
            "artifact_1",
            0,
            vec![terminal_append("committed")],
        )
        .expect("prepare committed prefix");
        let faults = FaultInjector::armed(JournalKind::Artifact, point);

        prepare_with_faults(
            root.path(),
            "task_1",
            "artifact_1",
            1,
            vec![terminal_append("invisible")],
            &faults,
        )
        .expect_err("armed artifact boundary must fail");
        assert!(!faults.pending(), "artifact/{point:?} was not reached");

        let visible = load(root.path(), "task_1", "artifact_1", 1)
            .expect("committed artifact prefix remains readable");
        assert_eq!(visible.terminal_outputs["terminal_1"], "committed");
    }
}

#[test]
fn reconciliation_preserves_artifact_bytes_for_an_unavailable_task() {
    let root = Builder::new()
        .prefix("artifact-unavailable-task")
        .tempdir()
        .expect("create state root");
    prepare(
        root.path(),
        "task_1",
        "artifact_1",
        0,
        vec![terminal_append("committed")],
    )
    .expect("prepare artifact frame");
    let path = artifact_path(root.path(), "task_1", "artifact_1").expect("artifact path");
    let original = fs::read(&path).expect("read original artifact bytes");
    let tasks = HashMap::from([(
        "task_1".to_string(),
        RecoveredTask::Unavailable {
            error: "quarantined".to_string(),
        },
    )]);

    reconcile(root.path(), &tasks).expect("skip unavailable Task reconciliation");

    assert_eq!(
        fs::read(path).expect("read preserved artifact bytes"),
        original
    );
}

fn terminal_append(data: &str) -> ArtifactOperation {
    ArtifactOperation::AppendTerminal {
        terminal_id: "terminal_1".to_string(),
        data: data.to_string(),
    }
}

#[test]
fn load_returns_the_appends_after_a_known_sequence_in_order() {
    let root = Builder::new()
        .prefix("artifact-appends-since")
        .tempdir()
        .expect("create state root");
    let append = |terminal: &str, data: &str| ArtifactOperation::AppendTerminal {
        terminal_id: terminal.to_string(),
        data: data.to_string(),
    };
    for (head, operations) in [
        (0, vec![append("terminal_a", "one")]),
        (
            1,
            vec![append("terminal_a", "two"), append("terminal_b", "x")],
        ),
        (2, vec![append("terminal_a", "three")]),
    ] {
        prepare(root.path(), "task_1", "artifact_1", head, operations)
            .expect("prepare artifact frame");
    }

    let (artifact, appends) =
        load_since(root.path(), "task_1", "artifact_1", 3, 1).expect("load after sequence 1");

    assert_eq!(artifact.revision, 3);
    assert_eq!(artifact.terminal_outputs["terminal_a"], "onetwothree");
    let appended: Vec<_> = appends
        .iter()
        .map(|append| (append.terminal_id.as_str(), append.data.as_str()))
        .collect();
    assert_eq!(
        appended,
        vec![
            ("terminal_a", "two"),
            ("terminal_b", "x"),
            ("terminal_a", "three")
        ]
    );
    let (_, none) = load_since(root.path(), "task_1", "artifact_1", 3, 3).expect("load at head");
    assert!(none.is_empty());
}
