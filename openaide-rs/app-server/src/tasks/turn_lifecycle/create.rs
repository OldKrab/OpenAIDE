use crate::agent::AgentConfigOptionsRequest;
use crate::protocol::errors::RuntimeError;
use crate::protocol::params::AgentConfigOptionsParams;
use crate::tasks::mutation::TaskCommitOptions;

use super::TaskTurnLifecycle;

mod adopted_session;
mod helpers;
mod prompt_start;

impl TaskTurnLifecycle {
    fn config_options(
        &self,
        params: AgentConfigOptionsParams,
    ) -> Result<crate::protocol::model::ConfigOptionsCatalog, RuntimeError> {
        self.agent_gateway
            .config_options(AgentConfigOptionsRequest {
                agent_id: params.agent_id,
                cwd: params.workspace_root,
            })
    }
}

fn create_snapshot_commit_options() -> TaskCommitOptions {
    TaskCommitOptions {
        refresh_message_history: false,
        response_snapshot_tail_limit: Some(100),
    }
}
