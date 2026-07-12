use super::ChatHistoryPolicy;

#[test]
fn product_default_task_snapshot_tail_limit_matches_render_budget() {
    assert_eq!(ChatHistoryPolicy::default().task_snapshot_tail_limit(), 100);
}
