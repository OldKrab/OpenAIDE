use crate::storage::records::TaskRecord;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VolatileRecoveryPlan {
    pub(crate) interrupt_active_turn: bool,
    pub(crate) mark_live_session_data_stale: bool,
    pub(crate) clear_pending_config_change: bool,
}

pub(crate) fn volatile_recovery_plan(task: &TaskRecord) -> Option<VolatileRecoveryPlan> {
    let plan = VolatileRecoveryPlan {
        interrupt_active_turn: task.active_turn_id.is_some(),
        mark_live_session_data_stale: (task.config_options_catalog.is_some()
            || task.agent_commands_catalog.is_some())
            && !task.native_session_data_freshness.is_stale(),
        clear_pending_config_change: task.config_mutation.pending.is_some(),
    };
    plan.has_work().then_some(plan)
}

impl VolatileRecoveryPlan {
    fn has_work(self) -> bool {
        self.interrupt_active_turn
            || self.mark_live_session_data_stale
            || self.clear_pending_config_change
    }
}

#[cfg(test)]
#[path = "task_recovery_tests.rs"]
mod tests;
