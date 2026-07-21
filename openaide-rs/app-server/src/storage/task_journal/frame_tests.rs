use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};

use serde::{Deserialize, Serialize};
use tempfile::Builder;

use super::{
    append, append_with_faults, create, create_with_faults, replace_with_faults, replay, scan,
    truncate_after, FaultInjector, FaultPoint, FramedRecord, JournalKind,
};

const SENTINEL_ROOT: &str = "SENSITIVE_ABSOLUTE_STATE_ROOT";

#[derive(Debug, Deserialize, Serialize)]
struct TestFrame {
    format_version: u16,
    sequence: u64,
}

fn frame(sequence: u64) -> TestFrame {
    TestFrame {
        format_version: 1,
        sequence,
    }
}

impl FramedRecord for TestFrame {
    fn format_version(&self) -> u16 {
        self.format_version
    }

    fn sequence(&self) -> u64 {
        self.sequence
    }
}

#[test]
fn replay_errors_never_disclose_the_state_root() {
    let root = Builder::new()
        .prefix(SENTINEL_ROOT)
        .tempdir()
        .expect("create sentinel state root");
    let path = root.path().join("task").join("journal.bin");
    create(
        &path,
        &TestFrame {
            format_version: 1,
            sequence: 1,
        },
    )
    .expect("create frame");

    let mut file = OpenOptions::new()
        .read(true)
        .write(true)
        .open(&path)
        .expect("open frame for corruption");
    file.seek(SeekFrom::End(-1)).expect("seek checksum");
    let mut checksum_byte = [0];
    file.read_exact(&mut checksum_byte).expect("read checksum");
    checksum_byte[0] ^= 0xff;
    file.seek(SeekFrom::End(-1)).expect("seek checksum again");
    file.write_all(&checksum_byte).expect("corrupt checksum");

    let message = match replay::<TestFrame>(&path) {
        Ok(_) => panic!("checksum damage must fail"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("checksum mismatch"));
    assert!(!message.contains(SENTINEL_ROOT));
    assert!(!message.contains(&root.path().display().to_string()));
}

#[test]
fn io_errors_report_stage_and_kind_without_disclosing_the_state_root() {
    let root = Builder::new()
        .prefix(SENTINEL_ROOT)
        .tempdir()
        .expect("create sentinel state root");
    let path = root.path().join("missing").join("journal.bin");

    let message = match replay::<TestFrame>(&path) {
        Ok(_) => panic!("missing journal must fail"),
        Err(error) => error.to_string(),
    };
    assert!(message.contains("replay_open"));
    assert!(message.contains("not_found"));
    assert!(!message.contains(SENTINEL_ROOT));
    assert!(!message.contains(&root.path().display().to_string()));
}

#[test]
fn scan_validates_and_indexes_frames_without_retaining_payloads() {
    let root = Builder::new()
        .prefix("journal-streaming-scan")
        .tempdir()
        .expect("create state root");
    let path = root.path().join("task").join("journal.bin");
    create(&path, &frame(1)).unwrap();
    append(&path, &frame(2)).unwrap();

    let scanned = scan::<TestFrame>(&path).expect("scan framed journal");

    assert!(scanned.frames.is_empty());
    assert_eq!(scanned.frame_count, 2);
    truncate_after(&path, &scanned, 1).expect("truncate from scan index");
    assert_eq!(replay::<TestFrame>(&path).unwrap().frames.len(), 1);
}

#[test]
fn every_append_boundary_restarts_as_an_exact_committed_prefix() {
    let points = [
        FaultPoint::AppendOpen,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
    ];
    for kind in [
        JournalKind::Task,
        JournalKind::Artifact,
        JournalKind::ArtifactReference,
    ] {
        for point in points {
            let root = Builder::new()
                .prefix("journal-append-fault")
                .tempdir()
                .expect("create state root");
            let path = root.path().join("task").join("journal.bin");
            create(&path, &frame(1)).expect("create first committed frame");
            let faults = FaultInjector::armed(kind, point);

            append_with_faults(&path, &frame(2), kind, &faults)
                .expect_err("armed boundary must fail");
            assert!(!faults.pending(), "{kind:?}/{point:?} was not reached");

            let replayed = replay::<TestFrame>(&path).expect("restart yields a valid prefix");
            let sequences = replayed
                .frames
                .iter()
                .map(|entry| entry.sequence)
                .collect::<Vec<_>>();
            assert!(
                sequences == [1] || sequences == [1, 2],
                "{kind:?}/{point:?} produced a non-prefix restart: {sequences:?}"
            );
        }
    }
}

#[test]
fn every_create_boundary_leaves_only_absent_incomplete_or_complete_state() {
    let points = [
        FaultPoint::DirectoryParentSync,
        FaultPoint::CreateOpen,
        FaultPoint::CreateHeaderWrite,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
    ];
    for kind in [
        JournalKind::Task,
        JournalKind::Artifact,
        JournalKind::ArtifactReference,
    ] {
        for point in points {
            let root = Builder::new()
                .prefix("journal-create-fault")
                .tempdir()
                .expect("create state root");
            let path = root.path().join("task").join("journal.bin");
            let faults = FaultInjector::armed(kind, point);

            create_with_faults(&path, &frame(1), kind, &faults)
                .expect_err("armed boundary must fail");
            assert!(!faults.pending(), "{kind:?}/{point:?} was not reached");

            if path.exists() {
                match replay::<TestFrame>(&path) {
                    Ok(replayed) => {
                        let sequences = replayed
                            .frames
                            .iter()
                            .map(|entry| entry.sequence)
                            .collect::<Vec<_>>();
                        assert!(
                            sequences.is_empty() || sequences == [1],
                            "{kind:?}/{point:?} published unexpected state: {sequences:?}"
                        );
                    }
                    Err(error) => assert!(
                        error.to_string().contains("Invalid Task journal header"),
                        "{kind:?}/{point:?} left unexpected corruption: {error}"
                    ),
                }
            }
        }
    }
}

#[test]
fn every_compaction_boundary_preserves_old_or_complete_replacement() {
    let points = [
        FaultPoint::CreateOpen,
        FaultPoint::CreateHeaderWrite,
        FaultPoint::FrameLengthWrite,
        FaultPoint::FramePayloadWrite,
        FaultPoint::FrameChecksumWrite,
        FaultPoint::FileSync,
        FaultPoint::ParentSync,
        FaultPoint::CompactionValidate,
        FaultPoint::CompactionPublish,
        FaultPoint::CompactionPublishParentSync,
    ];
    for point in points {
        let root = Builder::new()
            .prefix("journal-compaction-fault")
            .tempdir()
            .expect("create state root");
        let path = root.path().join("task").join("journal.bin");
        create(&path, &frame(1)).expect("create first frame");
        append(&path, &frame(2)).expect("append second frame");
        let faults = FaultInjector::armed(JournalKind::Compaction, point);

        replace_with_faults(&path, &frame(1), &faults)
            .expect_err("armed compaction boundary must fail");
        assert!(!faults.pending(), "compaction/{point:?} was not reached");

        let replayed = replay::<TestFrame>(&path).expect("canonical journal remains valid");
        let sequences = replayed
            .frames
            .iter()
            .map(|entry| entry.sequence)
            .collect::<Vec<_>>();
        assert!(
            sequences == [1, 2] || sequences == [1],
            "compaction/{point:?} produced partial publication: {sequences:?}"
        );
    }
}
