use super::changed_fields;
use crate::protocol::model::TaskBackgroundWork;
use crate::tasks::mutation::tests::task_record;

#[test]
fn background_work_alone_changes_the_summary() {
    let original = task_record("task_1");
    let mut held = original.clone();
    held.background_work = Some(TaskBackgroundWork {
        held: true,
        commands: Vec::new(),
    });

    assert!(changed_fields(&original, &held).summary);
    assert!(changed_fields(&held, &original).summary);
}
