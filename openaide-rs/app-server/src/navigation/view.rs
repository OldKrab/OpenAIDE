//! One client's Navigation view: its parameters, the window they select, and the diff against
//! what that client was last sent.

use std::collections::{BTreeMap, HashMap, HashSet};

use openaide_app_server_protocol::ids::ProjectId;
use openaide_app_server_protocol::navigation::{
    NavigationDiscovery, NavigationFrame, NavigationGroupFrame, NavigationRow, NavigationViewParams,
};
use openaide_app_server_protocol::task::TaskNavigationSection;

use crate::native_sessions::discovery_state::ProjectDiscovery;

use super::rows::{compare_groups, row_ref_id, ProjectRows, Row};

pub(super) struct View {
    pub(super) params: NavigationViewParams,
    sent: HashMap<ProjectId, GroupSent>,
    group_order: Vec<ProjectId>,
}

#[derive(PartialEq, Eq)]
struct GroupSent {
    label: String,
    order: Vec<String>,
    revisions: HashMap<String, u64>,
    total: u64,
    hidden: u64,
    undiscovered: bool,
    discovery: NavigationDiscovery,
}

/// A group as the view wants it, before comparing with what was sent.
struct Desired<'a> {
    project_id: &'a ProjectId,
    rows: &'a ProjectRows,
    window: Vec<(&'a Row, bool)>,
    state: GroupSent,
}

impl View {
    pub(super) fn new(params: NavigationViewParams) -> Self {
        Self {
            params,
            sent: HashMap::new(),
            group_order: Vec::new(),
        }
    }

    pub(super) fn section(&self) -> TaskNavigationSection {
        self.params.section
    }

    /// Forgets what was sent, so the next frame is a full baseline.
    pub(super) fn reset(&mut self) {
        self.sent.clear();
        self.group_order.clear();
    }

    /// The history limit this view applies to a Project.
    pub(super) fn history_limit(&self, project_id: &ProjectId) -> usize {
        self.params
            .history_limits
            .iter()
            .find(|limit| &limit.project_id == project_id)
            .map_or(self.params.history_limit, |limit| limit.history_limit) as usize
    }

    fn selects(&self, project_id: &ProjectId) -> bool {
        self.params
            .project_ids
            .as_ref()
            .is_none_or(|ids| ids.contains(project_id))
    }

    /// The rows of one Project this view shows: every live row, the first `history_limit`
    /// history rows, and the open row. Rows keep their sorted position. An idle open row keeps
    /// its own history slot and is added only when the window would hide it.
    fn window<'a>(&self, project_id: &ProjectId, rows: &'a ProjectRows) -> Vec<(&'a Row, bool)> {
        let limit = self.history_limit(project_id);
        let open = self.params.open_row.as_ref().map(row_ref_id);
        let mut history = 0usize;
        rows.rows
            .iter()
            .filter_map(|row| {
                let is_open = open.as_deref() == Some(row.id.as_str());
                let in_history_window = !row.live_by_state && {
                    history += 1;
                    history <= limit
                };
                (row.live_by_state || in_history_window || is_open)
                    .then_some((row, row.live_by_state || is_open))
            })
            .collect()
    }

    /// Diffs the view against `rows` and records the result as sent. `None` when the client
    /// already holds exactly this state.
    pub(super) fn diff(
        &mut self,
        rows: &HashMap<ProjectId, ProjectRows>,
        discovery: impl Fn(&ProjectId) -> ProjectDiscovery,
    ) -> Option<NavigationFrame> {
        let tasks = self.section() == TaskNavigationSection::Tasks;
        let mut selected: Vec<(&ProjectId, &ProjectRows)> =
            rows.iter().filter(|(id, _)| self.selects(id)).collect();
        selected.sort_by(|left, right| compare_groups(*left, *right));
        let desired: Vec<Desired<'_>> = selected
            .iter()
            .map(|(project_id, project_rows)| {
                let window = self.window(project_id, project_rows);
                let state = project_discovery(tasks, discovery(project_id));
                Desired {
                    project_id,
                    rows: project_rows,
                    state: GroupSent {
                        label: project_rows.label.clone(),
                        order: window.iter().map(|(row, _)| row.id.clone()).collect(),
                        revisions: window
                            .iter()
                            .map(|(row, live)| (row.id.clone(), revision(row, *live)))
                            .collect(),
                        total: project_rows.rows.len() as u64,
                        hidden: (project_rows.rows.len() - window.len()) as u64,
                        undiscovered: state.0,
                        discovery: state.1,
                    },
                    window,
                }
            })
            .collect();

        let group_order: Vec<ProjectId> = desired
            .iter()
            .map(|group| group.project_id.clone())
            .collect();
        let present: HashSet<&ProjectId> = group_order.iter().collect();
        let mut removed_groups: Vec<ProjectId> = self
            .sent
            .keys()
            .filter(|id| !present.contains(id))
            .cloned()
            .collect();
        removed_groups.sort();
        let mut groups = Vec::new();
        for group in desired {
            let previous = self.sent.get(group.project_id);
            if previous == Some(&group.state) {
                continue;
            }
            let bodies: BTreeMap<String, NavigationRow> = group
                .window
                .iter()
                .filter(|(row, live)| {
                    previous
                        .and_then(|sent| sent.revisions.get(&row.id))
                        .is_none_or(|sent| *sent != revision(row, *live))
                })
                .map(|(row, live)| {
                    (
                        row.id.clone(),
                        NavigationRow {
                            live: *live,
                            entry: row.entry.clone(),
                        },
                    )
                })
                .collect();
            groups.push(NavigationGroupFrame {
                project_id: group.project_id.clone(),
                project_label: group.rows.label.clone(),
                order: group.state.order.clone(),
                rows: bodies,
                total: group.state.total,
                hidden: group.state.hidden,
                undiscovered: group.state.undiscovered,
                discovery: group.state.discovery,
            });
            self.sent.insert(group.project_id.clone(), group.state);
        }
        for removed in &removed_groups {
            self.sent.remove(removed);
        }
        let order_changed = self.group_order != group_order;
        if groups.is_empty() && removed_groups.is_empty() && !order_changed {
            return None;
        }
        let frame = NavigationFrame {
            section: self.section(),
            // The owner stamps the revision.
            revision: 0,
            group_order: order_changed.then(|| group_order.clone()),
            removed_groups,
            groups,
        };
        self.group_order = group_order;
        Some(frame)
    }
}

/// The Archive never discovers, so its groups are always settled.
fn project_discovery(tasks: bool, discovery: ProjectDiscovery) -> (bool, NavigationDiscovery) {
    if !tasks {
        return (false, NavigationDiscovery::Settled);
    }
    let state = if discovery.loading {
        NavigationDiscovery::Loading
    } else if discovery.failed {
        NavigationDiscovery::Failed
    } else {
        NavigationDiscovery::Settled
    };
    (discovery.undiscovered, state)
}

fn revision(row: &Row, live: bool) -> u64 {
    row.hash ^ u64::from(live)
}
