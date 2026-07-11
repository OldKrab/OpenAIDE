use super::*;

#[test]
fn project_identity_normalizes_equivalent_workspace_roots() {
    let base = ProjectIdentity::from_workspace_root("/workspace/app");
    let with_segments = ProjectIdentity::from_workspace_root("/workspace/app/./src/..");
    let with_trailing_separator = ProjectIdentity::from_workspace_root("/workspace/app/");

    assert_eq!(base.project_id, with_segments.project_id);
    assert_eq!(base.project_id, with_trailing_separator.project_id);
    assert_eq!(with_segments.workspace_root, "/workspace/app");
    assert_eq!(with_segments.label, "app");
}

#[test]
fn empty_workspace_root_keeps_project_fallback_label() {
    let identity = ProjectIdentity::from_workspace_root("");

    assert_eq!(identity.workspace_root, "");
    assert_eq!(identity.label, "Project");
}

#[test]
fn project_identity_preserves_significant_whitespace() {
    let plain = ProjectIdentity::from_workspace_root("/workspace/app");
    let trailing_space = ProjectIdentity::from_workspace_root("/workspace/app ");

    assert_ne!(plain.project_id, trailing_space.project_id);
    assert_eq!(trailing_space.workspace_root, "/workspace/app ");
    assert_eq!(trailing_space.label, "app ");
}

#[test]
fn absolute_parent_segments_do_not_escape_above_root() {
    let root = ProjectIdentity::from_workspace_root("/");
    let above_root = ProjectIdentity::from_workspace_root("/..");
    let child = ProjectIdentity::from_workspace_root("/workspace");
    let child_with_parent_above_root = ProjectIdentity::from_workspace_root("/../workspace");

    assert_eq!(root.project_id, above_root.project_id);
    assert_eq!(child.project_id, child_with_parent_above_root.project_id);
    assert_eq!(child_with_parent_above_root.workspace_root, "/workspace");
}
