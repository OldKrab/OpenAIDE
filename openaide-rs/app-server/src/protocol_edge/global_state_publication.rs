//! Publishing the global state (ADR-0086): every handler or background publisher that may have
//! changed it names the areas, and the one owner decides what, if anything, each client is sent.

use openaide_app_server_protocol::client::ShellKind;
use openaide_app_server_protocol::global_state::{GlobalState, SyncNotification};
use openaide_app_server_protocol::worktree::WorktreeRepositorySnapshot;

use crate::client_lifecycle::ClientContext;
use crate::global_state::{GlobalAreas, GlobalStateAudience};

use super::RpcGateway;

impl RpcGateway {
    /// Who receives the global state and how much of it: VS Code only what concerns the Projects
    /// of its workspace, everyone else all of it.
    fn global_audience(&self, context: &ClientContext) -> GlobalStateAudience {
        GlobalStateAudience {
            client_instance_id: context.client_instance_id.clone(),
            project_filter: (context.shell.kind == ShellKind::VscodeExtension).then(|| {
                self.project_roots
                    .client_project_ids(&context.client_instance_id)
            }),
        }
    }

    /// The complete global state for a client's `client/initialize` reply.
    pub(super) fn global_state_for(
        &mut self,
        context: &ClientContext,
    ) -> Result<GlobalState, openaide_app_server_protocol::errors::ProtocolError> {
        let audience = self.global_audience(context);
        self.global_state.state_for(&self.snapshots, &audience)
    }

    /// Re-reads `areas` and sends each client one `sync` notification with what changed for it.
    /// Nothing is sent when nothing changed. The notifications join the outbox now, so they
    /// precede the reply of the request that caused them.
    pub(crate) fn publish_global_state(&mut self, areas: GlobalAreas) {
        self.publish_global_state_with(areas, None);
    }

    /// Publishes a worktree repository snapshot a handler or a background Git operation already
    /// holds; a repository equal to the one clients have is not published.
    pub(crate) fn publish_worktree_repository(&mut self, repository: WorktreeRepositorySnapshot) {
        self.publish_global_state_with(GlobalAreas::default(), Some(repository));
    }

    pub(crate) fn publish_global_state_with(
        &mut self,
        areas: GlobalAreas,
        repository: Option<WorktreeRepositorySnapshot>,
    ) {
        let audiences: Vec<_> = self
            .client_hub
            .client_contexts()
            .iter()
            .map(|context| self.global_audience(context))
            .collect();
        let deliveries = self
            .global_state
            .refresh(&self.snapshots, areas, repository, &audiences);
        for (client_instance_id, changes) in deliveries {
            if let Some(delivery) = self.client_hub.delivery_for(&client_instance_id) {
                self.outbox
                    .push_sync(&delivery.connection_id, SyncNotification { changes });
            }
        }
        if areas.has_projects_or_agents() {
            // Project membership, labels and enabled Agents are part of every Navigation view.
            self.navigation.mark_all();
        }
    }
}
