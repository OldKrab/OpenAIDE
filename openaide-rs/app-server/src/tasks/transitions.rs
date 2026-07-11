use crate::tasks::mutation::TaskMutations;

mod active_turn;
mod failure;
mod helpers;
mod recovery;

#[derive(Clone)]
pub(crate) struct TaskTransitions {
    mutations: TaskMutations,
}

impl TaskTransitions {
    pub(crate) fn new(mutations: TaskMutations) -> Self {
        Self { mutations }
    }
}
