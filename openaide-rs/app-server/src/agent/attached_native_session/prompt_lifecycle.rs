//! Prompt generations of one attachment: admission, steering requests and settlement.
//!
//! A generation is admitted for each primary prompt and retired when its settlement guard
//! drops; request guards tag ACP response waiters so a late response cannot touch a newer one.

use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::time::Duration;

use crate::agent::acp_active_prompt::PromptSettlementState;
use crate::agent::acp_process_lifetime::ProcessOperation;
use crate::agent::TurnCancellation;
use crate::protocol::errors::RuntimeError;

#[derive(Default)]
pub(super) struct PromptLifecycle {
    active: Mutex<Option<PromptGeneration>>,
    settled: Condvar,
    next_generation_id: AtomicU64,
}

struct PromptGeneration {
    id: u64,
    cancellation: TurnCancellation,
    settlement: Arc<PromptSettlementState>,
    accepting_steering: bool,
    outstanding_requests: usize,
}

impl PromptLifecycle {
    pub(super) fn admit(
        self: &Arc<Self>,
        cancellation: &TurnCancellation,
    ) -> Result<Option<PromptAdmission>, RuntimeError> {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        loop {
            match active.as_ref() {
                None => {
                    let generation_id = self.next_generation_id.fetch_add(1, Ordering::Relaxed) + 1;
                    let settlement = Arc::new(PromptSettlementState::default());
                    *active = Some(PromptGeneration {
                        id: generation_id,
                        cancellation: cancellation.clone(),
                        settlement: settlement.clone(),
                        accepting_steering: true,
                        outstanding_requests: 1,
                    });
                    return Ok(Some(PromptAdmission {
                        _settlement: PromptSettlementGuard {
                            lifecycle: self.clone(),
                            generation_id,
                        },
                        request: PromptRequestGuard {
                            lifecycle: self.clone(),
                            generation_id,
                            settlement,
                            process_operation: None,
                            steering_pending: false,
                        },
                    }));
                }
                Some(current)
                    if !current.accepting_steering
                        || current.cancellation.is_cancelled()
                        || current.settlement.kind().is_some() =>
                {
                    let (next_active, _) = self
                        .settled
                        .wait_timeout(active, Duration::from_millis(100))
                        .expect("ACP prompt lifecycle poisoned");
                    active = next_active;
                    if cancellation.is_cancelled() {
                        return Ok(None);
                    }
                }
                Some(_) => {
                    return Err(RuntimeError::NotReady(
                        "ACP session already has an active prompt".to_string(),
                    ));
                }
            }
        }
    }

    pub(super) fn register_steering_request(
        self: &Arc<Self>,
    ) -> Result<PromptRequestGuard, RuntimeError> {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active.as_mut().filter(|generation| {
            generation.accepting_steering && generation.settlement.kind().is_none()
        }) else {
            return Err(RuntimeError::NotReady(
                "ACP session has no active prompt to steer".to_string(),
            ));
        };
        // Record admission before enqueueing the command: the primary response
        // can race with steering dispatch on the shared ACP connection.
        if !generation.settlement.admit_steering() {
            return Err(RuntimeError::NotReady("ACP prompt already settled".into()));
        }
        generation.outstanding_requests += 1;
        Ok(PromptRequestGuard {
            lifecycle: self.clone(),
            generation_id: generation.id,
            settlement: generation.settlement.clone(),
            process_operation: None,
            steering_pending: true,
        })
    }

    fn finish_request(&self, generation_id: u64) {
        let mut active = self.active.lock().expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active
            .as_mut()
            .filter(|generation| generation.id == generation_id)
        else {
            return;
        };
        generation.outstanding_requests = generation
            .outstanding_requests
            .checked_sub(1)
            .expect("ACP prompt request count underflow");
    }
}

pub(super) struct PromptAdmission {
    _settlement: PromptSettlementGuard,
    pub(super) request: PromptRequestGuard,
}

struct PromptSettlementGuard {
    lifecycle: Arc<PromptLifecycle>,
    generation_id: u64,
}

impl Drop for PromptSettlementGuard {
    fn drop(&mut self) {
        let mut active = self
            .lifecycle
            .active
            .lock()
            .expect("ACP prompt lifecycle poisoned");
        let Some(generation) = active
            .as_mut()
            .filter(|generation| generation.id == self.generation_id)
        else {
            return;
        };
        generation.accepting_steering = false;
        let outstanding_requests = generation.outstanding_requests;
        active.take();
        self.lifecycle.settled.notify_all();
        crate::logging::info(
            "acp_prompt_generation_retired",
            serde_json::json!({
                "generation_id": self.generation_id,
                "outstanding_requests": outstanding_requests,
            }),
        );
    }
}

/// Tags an ACP response waiter so a late response cannot mutate a newer prompt generation.
pub(in crate::agent) struct PromptRequestGuard {
    process_operation: Option<ProcessOperation>,
    lifecycle: Arc<PromptLifecycle>,
    generation_id: u64,
    settlement: Arc<PromptSettlementState>,
    steering_pending: bool,
}

impl PromptRequestGuard {
    pub(super) fn with_process_operation(mut self, operation: ProcessOperation) -> Self {
        self.process_operation = Some(operation);
        self
    }

    pub(in crate::agent) fn settlement_state(&self) -> Arc<PromptSettlementState> {
        self.settlement.clone()
    }

    /// Delivery has been resolved or transferred to a tracked prompt. Until
    /// then extension-capable turns cannot retire ahead of an admitted Send.
    pub(in crate::agent) fn finish_steering_admission(&mut self) {
        if self.steering_pending {
            self.steering_pending = false;
            self.settlement.finish_steering();
        }
    }
}

impl Drop for PromptRequestGuard {
    fn drop(&mut self) {
        self.finish_steering_admission();
        self.lifecycle.finish_request(self.generation_id);
    }
}
