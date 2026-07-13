use crate::server_requests::ServerRequestRuntime;
use crate::tasks::mutation::TaskMutations;

mod active_turn;
mod active_work_end;
mod failure;
mod helpers;
mod recovery;

pub(crate) use active_work_end::ActiveWorkEnd;

#[derive(Clone)]
pub(crate) struct TaskTransitions {
    mutations: TaskMutations,
    server_requests: ServerRequestRuntime,
}

impl TaskTransitions {
    pub(crate) fn new(mutations: TaskMutations, server_requests: ServerRequestRuntime) -> Self {
        Self {
            mutations,
            server_requests,
        }
    }
}
