use crate::storage::records::TaskRecord;

pub(crate) const RESTART_INTERRUPTION_MESSAGE: &str =
    "Task was stopped because OpenAIDE restarted.";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct VolatileRecoveryPlan {
    pub(crate) interrupt_active_turn: bool,
}

pub(crate) fn volatile_recovery_plan(task: &TaskRecord) -> Option<VolatileRecoveryPlan> {
    let plan = VolatileRecoveryPlan {
        interrupt_active_turn: task.active_turn_id.is_some(),
    };
    plan.has_work().then_some(plan)
}

impl VolatileRecoveryPlan {
    fn has_work(self) -> bool {
        self.interrupt_active_turn
    }
}

#[cfg(test)]
mod tests;
