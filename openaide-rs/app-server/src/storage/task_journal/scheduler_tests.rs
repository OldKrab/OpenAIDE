use std::sync::mpsc;

use super::*;

#[test]
fn control_capacity_remains_when_one_task_fills_its_stream_lane() {
    let mut state = SchedulerState::default();
    for _ in 0..PER_TASK_STREAM_CAPACITY {
        let (reply, _receipt) = mpsc::channel();
        enqueue(&mut state, stream("noisy"), reply);
    }

    assert!(!has_capacity(&state, &stream("noisy")));
    assert!(has_capacity(&state, &TaskWrite::barrier("noisy")));
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

fn stream(task_id: &str) -> TaskWrite {
    TaskWrite::stream_append_terminal(task_id, "artifact", "terminal", "x")
}
