use std::sync::mpsc;
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use super::*;

#[test]
fn control_capacity_remains_when_one_task_fills_its_stream_lane() {
    let mut state = SchedulerState::default();
    let full_lane = stream_with_estimated_bytes("noisy", PER_TASK_STREAM_BYTE_CAPACITY);
    let (reply, _receipt) = mpsc::channel();
    enqueue(&mut state, full_lane, reply);

    assert!(!has_capacity(&state, &stream("noisy")));
    assert!(has_capacity(&state, &TaskWrite::barrier("noisy")));
}

#[test]
fn barrier_is_admitted_and_runs_after_a_full_stream_lane() {
    let scheduler = Scheduler::new();
    let (stream_reply, _stream_receipt) = mpsc::channel();
    scheduler
        .admit(
            stream_with_estimated_bytes("task", PER_TASK_STREAM_BYTE_CAPACITY),
            stream_reply,
        )
        .expect("fill task stream lane");
    let (barrier_reply, _barrier_receipt) = mpsc::channel();
    scheduler
        .admit(TaskWrite::barrier("task"), barrier_reply)
        .expect("control lane remains available");

    let NextWork::Batch { writes, .. } = scheduler.next() else {
        panic!("expected stream batch");
    };
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].write.boundary, CommitBoundary::Stream);

    let NextWork::Batch { writes, .. } = scheduler.next() else {
        panic!("expected barrier batch");
    };
    assert_eq!(writes.len(), 1);
    assert_eq!(writes[0].write.boundary, CommitBoundary::Barrier);
}

#[test]
fn stream_admission_is_bounded_by_actual_estimated_bytes() {
    let mut state = SchedulerState::default();
    let half_lane = PER_TASK_STREAM_BYTE_CAPACITY / 2;

    for task_id in ["one", "two"] {
        let write = stream_with_estimated_bytes(task_id, half_lane);
        assert!(has_capacity(&state, &write));
        let (reply, _receipt) = mpsc::channel();
        enqueue(&mut state, write, reply);
    }

    assert_eq!(state.global_stream_bytes, PER_TASK_STREAM_BYTE_CAPACITY);
    assert_eq!(
        state.pending["one"].stream_bytes,
        PER_TASK_STREAM_BYTE_CAPACITY / 2
    );

    let would_overfill_task =
        stream_with_estimated_bytes("one", PER_TASK_STREAM_BYTE_CAPACITY / 2 + 1);
    assert!(!has_capacity(&state, &would_overfill_task));
}

#[test]
fn queue_metrics_retain_global_and_per_task_high_water_marks_after_drain() {
    let scheduler = Scheduler::new();
    for (task_id, bytes) in [("one", 1_000), ("one", 2_000), ("two", 4_000)] {
        let (reply, _receipt) = mpsc::channel();
        scheduler
            .admit(stream_with_estimated_bytes(task_id, bytes), reply)
            .expect("admit measured stream write");
    }

    assert_eq!(
        scheduler.metrics(),
        SchedulerMetrics {
            peak_global_stream_bytes: 7_000,
            peak_task_stream_bytes: 4_000,
        }
    );
    assert!(matches!(scheduler.next(), NextWork::Batch { .. }));
    assert!(matches!(scheduler.next(), NextWork::Batch { .. }));
    assert_eq!(scheduler.metrics().peak_global_stream_bytes, 7_000);
}

#[test]
fn global_stream_admission_uses_the_sum_of_estimated_bytes() {
    let mut state = SchedulerState::default();
    let tasks_to_fill_global = GLOBAL_STREAM_BYTE_CAPACITY / PER_TASK_STREAM_BYTE_CAPACITY;
    for index in 0..tasks_to_fill_global {
        let write =
            stream_with_estimated_bytes(&format!("task-{index}"), PER_TASK_STREAM_BYTE_CAPACITY);
        assert!(has_capacity(&state, &write));
        let (reply, _receipt) = mpsc::channel();
        enqueue(&mut state, write, reply);
    }

    assert_eq!(state.global_stream_bytes, GLOBAL_STREAM_BYTE_CAPACITY);
    assert!(!has_capacity(&state, &stream("another-task")));
    assert!(has_capacity(&state, &TaskWrite::barrier("another-task")));
}

#[test]
fn draining_a_batch_releases_a_producer_blocked_by_byte_capacity() {
    let scheduler = Arc::new(Scheduler::new());
    let (first_reply, _first_receipt) = mpsc::channel();
    scheduler
        .admit(
            stream_with_estimated_bytes("task", PER_TASK_STREAM_BYTE_CAPACITY),
            first_reply,
        )
        .expect("fill task stream lane");

    let blocked_scheduler = scheduler.clone();
    let (attempt_started, started) = mpsc::channel();
    let (admission_result, result) = mpsc::channel();
    let producer = thread::spawn(move || {
        let (reply, _receipt) = mpsc::channel();
        attempt_started.send(()).expect("signal producer start");
        admission_result
            .send(blocked_scheduler.admit(stream("task"), reply))
            .expect("report admission result");
    });
    started.recv().expect("producer started");
    assert!(result.recv_timeout(Duration::from_millis(50)).is_err());

    let NextWork::Batch { task_id, writes } = scheduler.next() else {
        panic!("expected capacity-releasing batch");
    };
    assert_eq!(task_id, "task");
    assert_eq!(writes.len(), 1);
    assert!(result
        .recv_timeout(Duration::from_secs(1))
        .expect("producer released after drain")
        .is_ok());
    producer.join().expect("producer thread");
}

#[test]
fn shutdown_releases_blocked_producers_and_rejects_new_writes() {
    let scheduler = Arc::new(Scheduler::new());
    let (first_reply, _first_receipt) = mpsc::channel();
    scheduler
        .admit(
            stream_with_estimated_bytes("task", PER_TASK_STREAM_BYTE_CAPACITY),
            first_reply,
        )
        .expect("fill task stream lane");

    let blocked_scheduler = scheduler.clone();
    let (attempt_started, started) = mpsc::channel();
    let (admission_result, result) = mpsc::channel();
    let producer = thread::spawn(move || {
        let (reply, _receipt) = mpsc::channel();
        attempt_started.send(()).expect("signal producer start");
        admission_result
            .send(blocked_scheduler.admit(stream("task"), reply))
            .expect("report admission result");
    });
    started.recv().expect("producer started");
    assert!(result.recv_timeout(Duration::from_millis(50)).is_err());

    let (shutdown_reply, _shutdown_receipt) = mpsc::channel();
    scheduler
        .request_shutdown(shutdown_reply)
        .expect("request shutdown");
    assert!(result
        .recv_timeout(Duration::from_secs(1))
        .expect("blocked producer released")
        .is_err());
    let (reply, _receipt) = mpsc::channel();
    assert!(scheduler.admit(stream("new-task"), reply).is_err());
    producer.join().expect("producer thread");
}

#[test]
fn stream_write_larger_than_a_lane_is_rejected_instead_of_waiting_forever() {
    let scheduler = Scheduler::new();
    let oversized =
        stream_with_estimated_bytes("task", PER_TASK_STREAM_BYTE_CAPACITY.saturating_add(1));
    let (reply, _receipt) = mpsc::channel();

    assert!(scheduler.admit(oversized, reply).is_err());
}

#[test]
fn noisy_task_gets_only_one_batch_before_an_interactive_task() {
    let scheduler = Scheduler::new();
    for _ in 0..300 {
        let (reply, _receipt) = mpsc::channel();
        scheduler
            .admit(stream("noisy"), reply)
            .expect("admit noise");
    }
    let (reply, _receipt) = mpsc::channel();
    scheduler
        .admit(stream("interactive"), reply)
        .expect("admit interactive write");

    let NextWork::Batch { task_id, writes } = scheduler.next() else {
        panic!("expected noisy batch");
    };
    assert_eq!(task_id, "noisy");
    assert_eq!(writes.len(), MAX_BATCH_OPERATIONS);

    let NextWork::Batch { task_id, writes } = scheduler.next() else {
        panic!("expected interactive batch");
    };
    assert_eq!(task_id, "interactive");
    assert_eq!(writes.len(), 1);
}

#[test]
fn barrier_seals_earlier_same_task_stream_writes_in_one_ordered_batch() {
    let scheduler = Scheduler::new();
    let (stream_reply, _stream_receipt) = mpsc::channel();
    scheduler
        .admit(stream("task"), stream_reply)
        .expect("admit stream");
    let (barrier_reply, _barrier_receipt) = mpsc::channel();
    scheduler
        .admit(TaskWrite::barrier("task"), barrier_reply)
        .expect("admit barrier");

    let NextWork::Batch { writes, .. } = scheduler.next() else {
        panic!("expected ordered batch");
    };
    assert_eq!(writes.len(), 2);
    assert_eq!(writes[0].write.boundary, CommitBoundary::Stream);
    assert_eq!(writes[1].write.boundary, CommitBoundary::Barrier);
}

#[test]
fn round_robin_fairness_repeats_across_multiple_batches() {
    let scheduler = Scheduler::new();
    for task_id in ["one", "two", "three"] {
        for _ in 0..(MAX_BATCH_OPERATIONS + 1) {
            let (reply, _receipt) = mpsc::channel();
            scheduler
                .admit(stream(task_id), reply)
                .expect("admit stream write");
        }
    }

    for expected_task in ["one", "two", "three", "one", "two", "three"] {
        let NextWork::Batch { task_id, .. } = scheduler.next() else {
            panic!("expected fair batch");
        };
        assert_eq!(task_id, expected_task);
    }
}

fn stream(task_id: &str) -> TaskWrite {
    TaskWrite::stream_append_terminal(task_id, "artifact", "terminal", "x")
}

fn stream_with_estimated_bytes(task_id: &str, estimated_bytes: usize) -> TaskWrite {
    let baseline =
        TaskWrite::stream_append_terminal(task_id, "artifact", "terminal", "").estimated_bytes();
    assert!(
        estimated_bytes >= baseline,
        "test payload is below overhead"
    );
    let write = TaskWrite::stream_append_terminal(
        task_id,
        "artifact",
        "terminal",
        "x".repeat(estimated_bytes - baseline),
    );
    assert_eq!(write.estimated_bytes(), estimated_bytes);
    write
}
