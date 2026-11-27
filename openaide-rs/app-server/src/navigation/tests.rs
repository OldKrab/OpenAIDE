use std::collections::{HashMap, HashSet};
use std::sync::{Arc, Mutex};

use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::ids::{ClientInstanceId, ProjectId};
use openaide_app_server_protocol::navigation::{
    NavigationDiscovery, NavigationFrame, NavigationHistoryLimit, NavigationRowRef,
    NavigationViewParams,
};
use openaide_app_server_protocol::snapshot::{
    NativeSessionReference, NativeSessionSummary, TaskAttentionEvent, TaskAttentionReason,
    TaskLifecycle, TaskNavigationEntry, TaskStatus, TaskSummary,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use super::*;
use crate::native_sessions::discovery_state::{DiscoveryState, ProjectDiscovery};

/// A Project label with its entries.
type ProjectFixture = (String, Vec<TaskNavigationEntry>);

#[derive(Default)]
struct MockSource {
    rows: Mutex<HashMap<(TaskNavigationSection, String), ProjectFixture>>,
    discovery: Mutex<HashMap<String, ProjectDiscovery>>,
    reads: Mutex<Vec<Option<HashSet<ProjectId>>>>,
}

impl MockSource {
    fn set(
        &self,
        section: TaskNavigationSection,
        project: &str,
        entries: Vec<TaskNavigationEntry>,
    ) {
        self.rows.lock().unwrap().insert(
            (section, project.to_string()),
            (format!("{project} label"), entries),
        );
    }

    fn set_discovery(&self, project: &str, discovery: ProjectDiscovery) {
        self.discovery
            .lock()
            .unwrap()
            .insert(project.to_string(), discovery);
    }

    fn reads(&self) -> Vec<Option<HashSet<ProjectId>>> {
        self.reads.lock().unwrap().clone()
    }
}

impl NavigationSource for MockSource {
    fn groups(
        &self,
        section: TaskNavigationSection,
        projects: Option<&HashSet<ProjectId>>,
    ) -> Result<Vec<SourceGroup>, ProtocolError> {
        self.reads.lock().unwrap().push(projects.cloned());
        Ok(self
            .rows
            .lock()
            .unwrap()
            .iter()
            .filter(|((candidate, project), _)| {
                *candidate == section
                    && projects.is_none_or(|set| set.contains(&ProjectId::from(project.clone())))
            })
            .map(|((_, project), (label, entries))| SourceGroup {
                project_id: ProjectId::from(project.clone()),
                label: label.clone(),
                entries: entries.clone(),
            })
            .collect())
    }

    fn discovery(&self, project_id: &ProjectId) -> ProjectDiscovery {
        self.discovery
            .lock()
            .unwrap()
            .get(project_id.as_str())
            .copied()
            .unwrap_or_default()
    }
}

struct Harness {
    source: Arc<MockSource>,
    dirty: NavigationDirty,
    discovery: DiscoveryState,
    owner: NavigationOwner,
    client: ClientInstanceId,
}

fn harness() -> Harness {
    let source = Arc::new(MockSource::default());
    let dirty = NavigationDirty::default();
    let discovery = DiscoveryState::default();
    Harness {
        owner: NavigationOwner::new(source.clone(), dirty.clone(), discovery.clone()),
        source,
        dirty,
        discovery,
        client: ClientInstanceId::from("client-1"),
    }
}

fn params(limit: u32) -> NavigationViewParams {
    NavigationViewParams {
        section: TaskNavigationSection::Tasks,
        project_ids: None,
        history_limit: limit,
        history_limits: Vec::new(),
        open_row: None,
    }
}

fn task(id: &str, activity: u32) -> TaskSummary {
    TaskSummary {
        task_id: id.into(),
        project_id: "p1".into(),
        agent_id: "agent".into(),
        lifecycle: TaskLifecycle::Open,
        title: None,
        status: TaskStatus::Idle,
        updated_at: activity.to_string(),
        last_activity: activity.to_string(),
        unread: false,
        pinned: false,
        attention: None,
        has_messages: true,
        worktree_id: None,
        workspace_available: true,
    }
}

fn entry(task: TaskSummary) -> TaskNavigationEntry {
    TaskNavigationEntry::Task {
        task: Box::new(task),
    }
}

fn session(id: &str, activity: u32, pinned: bool) -> TaskNavigationEntry {
    TaskNavigationEntry::NativeSession {
        session: NativeSessionSummary {
            reference: NativeSessionReference {
                agent_id: "agent".into(),
                session_id: id.to_string(),
            },
            project_id: "p1".into(),
            workspace_root: "/work".to_string(),
            worktree_id: None,
            title: None,
            pinned,
            last_activity: Some(activity.to_string()),
        },
    }
}

fn status(mut task: TaskSummary, status: TaskStatus) -> TaskSummary {
    task.status = status;
    task
}

/// The baseline for the harness client, taken the way `navigation/subscribe` takes it.
fn subscribe(h: &mut Harness, params: NavigationViewParams) -> NavigationFrame {
    let mut flushed = h.owner.upsert_view(&h.client, params, true).unwrap();
    flushed.frames.remove(0).1
}

fn order(frame: &NavigationFrame, project: &str) -> Vec<String> {
    frame
        .groups
        .iter()
        .find(|group| group.project_id.as_str() == project)
        .unwrap_or_else(|| panic!("no group {project} in {frame:?}"))
        .order
        .clone()
}

fn tasks_with(n: u32) -> Vec<TaskNavigationEntry> {
    (1..=n).map(|i| entry(task(&format!("t{i}"), i))).collect()
}

#[test]
fn baseline_carries_group_order_bodies_totals_and_hidden() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(5));

    let frame = subscribe(&mut h, params(2));

    assert_eq!(frame.group_order, Some(vec![ProjectId::from("p1")]));
    let group = &frame.groups[0];
    assert_eq!(group.order, vec!["task:t5", "task:t4"]);
    assert_eq!(group.rows.len(), 2);
    assert_eq!((group.total, group.hidden), (5, 3));
}

#[test]
fn a_recompute_that_changes_nothing_publishes_nothing() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(3));
    subscribe(&mut h, params(7));

    // A catalog page that only re-observes known sessions marks the Project dirty.
    h.dirty.mark_project("p1".into());

    assert!(h.owner.flush().frames.is_empty());
}

#[test]
fn changed_row_sends_only_its_body_and_the_full_order() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(3));
    subscribe(&mut h, params(7));
    let mut rows = tasks_with(3);
    rows[0] = entry(task("t1", 10));
    h.source.set(TaskNavigationSection::Tasks, "p1", rows);
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    let group = &frames[0].1.groups[0];
    assert_eq!(group.order, vec!["task:t1", "task:t3", "task:t2"]);
    assert_eq!(group.rows.keys().collect::<Vec<_>>(), vec!["task:t1"]);
    assert!(frames[0].1.group_order.is_none());
}

#[test]
fn live_rows_outside_the_limit_stay_and_do_not_use_history_slots() {
    let mut h = harness();
    let mut rows = tasks_with(6);
    rows[0] = entry(status(task("t1", 1), TaskStatus::Waiting));
    rows[1] = entry({
        let mut unread = task("t2", 2);
        unread.unread = true;
        unread
    });
    rows[2] = entry({
        let mut attention = task("t3", 3);
        attention.attention = Some(TaskAttentionEvent {
            event_id: "e".to_string(),
            reason: TaskAttentionReason::Finished,
            occurred_at: "3".to_string(),
        });
        attention
    });
    h.source.set(TaskNavigationSection::Tasks, "p1", rows);

    let frame = subscribe(&mut h, params(2));

    // t6 and t5 are the two history rows; t1 to t3 are live and stay although older.
    assert_eq!(
        order(&frame, "p1"),
        vec!["task:t6", "task:t5", "task:t3", "task:t2", "task:t1"]
    );
    let group = &frame.groups[0];
    assert!(group.rows["task:t1"].live && !group.rows["task:t6"].live);
    assert_eq!(group.hidden, 1);
}

#[test]
fn idle_open_row_keeps_its_history_slot_and_is_added_when_the_window_hides_it() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(5));
    let mut open_inside = params(3);
    open_inside.open_row = Some(NavigationRowRef::Task {
        task_id: "t4".into(),
    });
    let inside = subscribe(&mut h, open_inside);
    assert_eq!(order(&inside, "p1"), vec!["task:t5", "task:t4", "task:t3"]);

    let mut open_outside = params(3);
    open_outside.open_row = Some(NavigationRowRef::Task {
        task_id: "t1".into(),
    });
    h.owner.remove_view(&h.client, TaskNavigationSection::Tasks);
    let outside = subscribe(&mut h, open_outside);
    assert_eq!(
        order(&outside, "p1"),
        vec!["task:t5", "task:t4", "task:t3", "task:t1"]
    );
    assert!(outside.groups[0].rows["task:t1"].live);
}

#[test]
fn one_comparator_ranks_pinned_native_sessions_with_tasks() {
    let mut h = harness();
    h.source.set(
        TaskNavigationSection::Tasks,
        "p1",
        vec![
            entry(task("t1", 50)),
            session("s-old-pinned", 1, true),
            entry(status(task("t2", 5), TaskStatus::Running)),
            session("s-new", 60, false),
        ],
    );

    let frame = subscribe(&mut h, params(10));

    assert_eq!(
        order(&frame, "p1"),
        vec![
            "session:agent:s-old-pinned",
            "task:t2",
            "session:agent:s-new",
            "task:t1"
        ]
    );
    // A pinned Native Session is live like a pinned Task.
    assert!(frame.groups[0].rows["session:agent:s-old-pinned"].live);
}

#[test]
fn groups_with_running_work_rank_first_then_by_activity_then_label() {
    let mut h = harness();
    let mut running = task("run", 1);
    running.project_id = "p3".into();
    running.status = TaskStatus::Running;
    h.source.set(
        TaskNavigationSection::Tasks,
        "p1",
        vec![entry(task("a", 9))],
    );
    h.source.set(
        TaskNavigationSection::Tasks,
        "p2",
        vec![entry(task("b", 20))],
    );
    h.source
        .set(TaskNavigationSection::Tasks, "p3", vec![entry(running)]);

    let frame = subscribe(&mut h, params(5));

    assert_eq!(
        frame.group_order,
        Some(vec!["p3".into(), "p2".into(), "p1".into()])
    );
}

#[test]
fn a_burst_of_marks_costs_one_recompute_of_only_the_dirty_projects() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(2));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(2));
    subscribe(&mut h, params(5));
    let before = h.source.reads().len();

    for _ in 0..50 {
        h.dirty.mark_project("p2".into());
    }
    h.owner.flush();

    let reads = h.source.reads();
    assert_eq!(reads.len(), before + 1);
    assert_eq!(
        reads.last().unwrap(),
        &Some(HashSet::from([ProjectId::from("p2")]))
    );
}

#[test]
fn project_and_agent_changes_rebuild_every_project() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(2));
    subscribe(&mut h, params(5));

    h.dirty.mark_all();
    h.owner.flush();

    assert_eq!(h.source.reads().last().unwrap(), &None);
}

#[test]
fn discovery_progress_is_a_per_group_frame_without_row_bodies() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(2));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(2));
    subscribe(&mut h, params(5));
    h.source.set_discovery(
        "p2",
        ProjectDiscovery {
            loading: true,
            undiscovered: true,
            failed: false,
        },
    );
    h.dirty.mark_project("p2".into());

    let frames = h.owner.flush().frames;

    let frame = &frames[0].1;
    assert_eq!(frame.groups.len(), 1);
    let group = &frame.groups[0];
    assert_eq!(group.project_id.as_str(), "p2");
    assert_eq!(group.discovery, NavigationDiscovery::Loading);
    assert!(group.undiscovered && group.rows.is_empty());
}

#[test]
fn a_failed_discovery_is_reported_for_its_group_only() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(1));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(1));
    subscribe(&mut h, params(5));
    h.source.set_discovery(
        "p1",
        ProjectDiscovery {
            failed: true,
            ..Default::default()
        },
    );
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    assert_eq!(frames[0].1.groups.len(), 1);
    assert_eq!(frames[0].1.groups[0].discovery, NavigationDiscovery::Failed);
}

#[test]
fn project_ids_limit_the_groups_of_a_view() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(1));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(1));
    let mut vscode = params(5);
    vscode.project_ids = Some(vec!["p2".into()]);

    let frame = subscribe(&mut h, vscode);

    assert_eq!(frame.group_order, Some(vec![ProjectId::from("p2")]));
    assert_eq!(frame.groups.len(), 1);
    // Only the shown Project drives discovery.
    assert_eq!(h.discovery.target("p2"), Some(5));
    assert_eq!(h.discovery.target("p1"), None);
}

#[test]
fn archive_view_never_drives_discovery_and_reports_settled() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Archive, "p1", tasks_with(2));
    h.source.set_discovery(
        "p1",
        ProjectDiscovery {
            loading: true,
            undiscovered: true,
            failed: false,
        },
    );
    let mut archive = params(20);
    archive.section = TaskNavigationSection::Archive;

    let frame = subscribe(&mut h, archive);

    assert_eq!(h.discovery.target("p1"), None);
    assert_eq!(frame.groups[0].discovery, NavigationDiscovery::Settled);
    assert!(!frame.groups[0].undiscovered);
}

#[test]
fn raising_a_history_limit_asks_discovery_to_deepen_and_lowering_does_not() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(30));
    let first = h.owner.upsert_view(&h.client, params(7), true).unwrap();
    assert_eq!(first.deepen, vec![(ProjectId::from("p1"), 7)]);

    let mut larger = params(7);
    larger.history_limits = vec![NavigationHistoryLimit {
        project_id: "p1".into(),
        history_limit: 17,
    }];
    let grown = h.owner.upsert_view(&h.client, larger, false).unwrap();
    assert_eq!(grown.deepen, vec![(ProjectId::from("p1"), 17)]);
    // The new window is an ordinary diff frame, not a baseline.
    assert_eq!(grown.frames[0].1.groups[0].order.len(), 17);
    assert!(grown.frames[0].1.group_order.is_none());

    let shrunk = h.owner.upsert_view(&h.client, params(3), false).unwrap();
    assert!(shrunk.deepen.is_empty());
    assert_eq!(shrunk.frames[0].1.groups[0].order.len(), 3);
}

#[test]
fn removing_a_group_is_reported() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(1));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(1));
    subscribe(&mut h, params(5));
    h.source
        .rows
        .lock()
        .unwrap()
        .remove(&(TaskNavigationSection::Tasks, "p2".to_string()));
    h.dirty.mark_project("p2".into());

    let frames = h.owner.flush().frames;

    assert_eq!(frames[0].1.removed_groups, vec![ProjectId::from("p2")]);
    assert_eq!(frames[0].1.group_order, Some(vec![ProjectId::from("p1")]));
}

#[test]
fn every_view_of_every_client_gets_its_own_diff_in_one_flush() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(3));
    subscribe(&mut h, params(1));
    h.owner
        .upsert_view(&ClientInstanceId::from("client-2"), params(3), true)
        .unwrap();
    h.source.set(TaskNavigationSection::Tasks, "p1", {
        let mut rows = tasks_with(3);
        rows.push(entry(task("t9", 99)));
        rows
    });
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    assert_eq!(frames.len(), 2);
    let by_client: HashMap<_, _> = frames.into_iter().collect();
    assert_eq!(by_client[&h.client].groups[0].order, vec!["task:t9"]);
    assert_eq!(
        by_client[&ClientInstanceId::from("client-2")].groups[0].order,
        vec!["task:t9", "task:t3", "task:t2"]
    );
}

#[test]
fn adopting_a_native_session_swaps_its_identity_in_one_frame() {
    let mut h = harness();
    h.source.set(
        TaskNavigationSection::Tasks,
        "p1",
        vec![session("s1", 5, false), entry(task("t1", 1))],
    );
    subscribe(&mut h, params(5));
    // The session becomes the Task that owns it.
    h.source.set(
        TaskNavigationSection::Tasks,
        "p1",
        vec![entry(task("adopted", 5)), entry(task("t1", 1))],
    );
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    assert_eq!(frames.len(), 1);
    assert_eq!(
        order(&frames[0].1, "p1"),
        vec!["task:adopted", "task:t1"],
        "the session id leaves and the Task id enters in the same frame"
    );
    assert!(frames[0].1.groups[0].rows.contains_key("task:adopted"));
}

#[test]
fn archiving_moves_a_row_between_sections_in_one_batch() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(2));
    h.source
        .set(TaskNavigationSection::Archive, "p1", Vec::new());
    subscribe(&mut h, params(5));
    let mut archive = params(5);
    archive.section = TaskNavigationSection::Archive;
    h.owner.upsert_view(&h.client, archive, true).unwrap();
    h.source.set(
        TaskNavigationSection::Tasks,
        "p1",
        vec![entry(task("t1", 1))],
    );
    h.source.set(
        TaskNavigationSection::Archive,
        "p1",
        vec![entry(task("t2", 2))],
    );
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    let by_section: HashMap<_, _> = frames
        .iter()
        .map(|(_, frame)| (frame.section, frame))
        .collect();
    assert_eq!(
        order(by_section[&TaskNavigationSection::Tasks], "p1"),
        ["task:t1"]
    );
    assert_eq!(
        order(by_section[&TaskNavigationSection::Archive], "p1"),
        ["task:t2"],
        "the row leaves one section and enters the other in the same recompute"
    );
}

#[test]
fn a_task_attention_event_keeps_a_row_in_the_window_whatever_the_limit() {
    let mut h = harness();
    let mut waiting = task("old", 1);
    waiting.attention = Some(TaskAttentionEvent {
        event_id: "attention-1".to_string(),
        reason: TaskAttentionReason::NeedsPermission,
        occurred_at: "1".to_string(),
    });
    let mut rows = tasks_with(4);
    rows.push(entry(waiting));
    h.source.set(TaskNavigationSection::Tasks, "p1", rows);

    let frame = subscribe(&mut h, params(1));

    let ids = order(&frame, "p1");
    assert!(ids.contains(&"task:old".to_string()), "{ids:?}");
    assert!(frame.groups[0].rows["task:old"].live);
    assert_eq!(frame.groups[0].hidden, 3);
}

#[test]
fn a_pin_changes_body_and_position_in_one_frame() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(3));
    subscribe(&mut h, params(5));
    let mut rows = tasks_with(3);
    rows[0] = entry(TaskSummary {
        pinned: true,
        ..task("t1", 1)
    });
    h.source.set(TaskNavigationSection::Tasks, "p1", rows);
    h.dirty.mark_project("p1".into());

    let frames = h.owner.flush().frames;

    let group = &frames[0].1.groups[0];
    assert_eq!(group.order, vec!["task:t1", "task:t3", "task:t2"]);
    assert_eq!(group.rows.len(), 1, "only the changed row travels");
    assert!(group.rows["task:t1"].live);
}

#[test]
fn collapse_and_show_more_move_the_window_and_the_hidden_count() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(9));
    subscribe(&mut h, params(7));

    let collapsed = h.owner.upsert_view(&h.client, params(3), false).unwrap();
    let group = &collapsed.frames[0].1.groups[0];
    assert_eq!((group.order.len(), group.total, group.hidden), (3, 9, 6));
    assert!(group.rows.is_empty(), "known rows are not resent");

    let more = h.owner.upsert_view(&h.client, params(9), false).unwrap();
    let group = &more.frames[0].1.groups[0];
    assert_eq!((group.order.len(), group.hidden), (9, 0));
    assert_eq!(
        group.rows.len(),
        6,
        "only the rows new to the client travel"
    );
}

#[test]
fn changing_the_project_ids_swaps_the_groups_of_a_view() {
    let mut h = harness();
    h.source
        .set(TaskNavigationSection::Tasks, "p1", tasks_with(1));
    h.source
        .set(TaskNavigationSection::Tasks, "p2", tasks_with(1));
    let mut folder = params(5);
    folder.project_ids = Some(vec!["p1".into()]);
    subscribe(&mut h, folder);

    let mut other = params(5);
    other.project_ids = Some(vec!["p2".into()]);
    let flushed = h.owner.upsert_view(&h.client, other, false).unwrap();

    let frame = &flushed.frames[0].1;
    assert_eq!(frame.group_order, Some(vec![ProjectId::from("p2")]));
    assert_eq!(frame.removed_groups, vec![ProjectId::from("p1")]);
    assert_eq!(frame.groups[0].project_id.as_str(), "p2");
}
