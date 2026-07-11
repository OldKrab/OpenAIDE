use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Condvar, Mutex};

use crate::agent::acp_host_terminal_cleanup::{
    kill_host_terminal, release_host_terminal, wait_for_host_terminal_exit,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::host::HostBridge;

static NEXT_OWNER_ID: AtomicU64 = AtomicU64::new(1);

#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
pub(super) struct AcpTerminalOwnerId(u64);

impl AcpTerminalOwnerId {
    pub(super) fn next() -> Self {
        Self(NEXT_OWNER_ID.fetch_add(1, Ordering::Relaxed))
    }
}

/// Owns host terminal handles for one shared ACP process.
///
/// An owner token exists before ACP returns a session ID, so failed and timed-out opens can clean
/// up terminals too. Cleanup closes admission first, waits for in-flight creates to report their
/// handles, then drains every recorded handle.
#[derive(Clone)]
pub(super) struct AcpHostTerminalRegistry {
    inner: Arc<TerminalRegistryInner>,
}

struct TerminalRegistryInner {
    host_bridge: HostBridge,
    state: Mutex<TerminalRegistryState>,
    changed: Condvar,
}

#[derive(Default)]
struct TerminalRegistryState {
    current_open: Option<AcpTerminalOwnerId>,
    owners: HashMap<AcpTerminalOwnerId, TerminalOwnerState>,
    session_owners: HashMap<String, AcpTerminalOwnerId>,
}

struct TerminalOwnerState {
    phase: TerminalOwnerPhase,
    in_flight_creates: usize,
    cleanup_in_progress: bool,
    terminals: HashSet<OwnedTerminal>,
}

#[derive(Clone, Copy, Eq, PartialEq)]
enum TerminalOwnerPhase {
    Opening,
    Active,
    Cancelled,
    Closed,
}

#[derive(Clone, Debug, Eq, Hash, PartialEq)]
struct OwnedTerminal {
    session_id: String,
    terminal_id: String,
}

#[derive(Clone)]
pub(super) struct AcpTerminalOwner {
    registry: AcpHostTerminalRegistry,
    id: AcpTerminalOwnerId,
}

pub(super) struct AcpTerminalCreatePermit {
    registry: AcpHostTerminalRegistry,
    owner_id: AcpTerminalOwnerId,
    session_id: String,
    completed: bool,
}

impl AcpHostTerminalRegistry {
    pub(super) fn new(host_bridge: HostBridge) -> Self {
        Self {
            inner: Arc::new(TerminalRegistryInner {
                host_bridge,
                state: Mutex::new(TerminalRegistryState::default()),
                changed: Condvar::new(),
            }),
        }
    }

    pub(super) fn owner(&self, id: AcpTerminalOwnerId) -> AcpTerminalOwner {
        AcpTerminalOwner {
            registry: self.clone(),
            id,
        }
    }

    pub(super) fn begin_open(&self, owner_id: AcpTerminalOwnerId) {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        state.owners.entry(owner_id).or_insert(TerminalOwnerState {
            phase: TerminalOwnerPhase::Opening,
            in_flight_creates: 0,
            cleanup_in_progress: false,
            terminals: HashSet::new(),
        });
        state.current_open = Some(owner_id);
    }

    pub(super) fn begin_create(
        &self,
        session_id: &str,
    ) -> Result<AcpTerminalCreatePermit, RuntimeError> {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        let owner_id = state
            .session_owners
            .get(session_id)
            .copied()
            .or(state.current_open)
            .ok_or_else(|| {
                RuntimeError::NotReady("ACP terminal session is not active".to_string())
            })?;
        let phase = state
            .owners
            .get(&owner_id)
            .map(|owner| owner.phase)
            .ok_or_else(|| RuntimeError::NotReady("ACP terminal owner is missing".to_string()))?;
        if !matches!(
            phase,
            TerminalOwnerPhase::Opening | TerminalOwnerPhase::Active
        ) {
            return Err(RuntimeError::NotReady(
                "ACP terminal session is cancelled".to_string(),
            ));
        }
        state
            .session_owners
            .entry(session_id.to_string())
            .or_insert(owner_id);
        state
            .owners
            .get_mut(&owner_id)
            .expect("terminal owner disappeared")
            .in_flight_creates += 1;
        Ok(AcpTerminalCreatePermit {
            registry: self.clone(),
            owner_id,
            session_id: session_id.to_string(),
            completed: false,
        })
    }

    pub(super) fn released(&self, session_id: &str, terminal_id: &str) {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        let Some(owner_id) = state.session_owners.get(session_id).copied() else {
            return;
        };
        if let Some(owner) = state.owners.get_mut(&owner_id) {
            owner.terminals.remove(&OwnedTerminal {
                session_id: session_id.to_string(),
                terminal_id: terminal_id.to_string(),
            });
        }
    }

    pub(super) fn close_all(&self) {
        let owner_ids = self
            .inner
            .state
            .lock()
            .expect("terminal registry poisoned")
            .owners
            .keys()
            .copied()
            .collect::<Vec<_>>();
        for owner_id in owner_ids {
            let _ = self.cleanup(owner_id, TerminalOwnerPhase::Closed);
        }
    }

    fn activate_session(&self, owner_id: AcpTerminalOwnerId, session_id: &str) {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        if let Some(owner) = state.owners.get_mut(&owner_id) {
            if matches!(
                owner.phase,
                TerminalOwnerPhase::Cancelled | TerminalOwnerPhase::Closed
            ) {
                return;
            }
            owner.phase = TerminalOwnerPhase::Active;
            state
                .session_owners
                .insert(session_id.to_string(), owner_id);
            if state.current_open == Some(owner_id) {
                state.current_open = None;
            }
        }
    }

    fn activate(&self, owner_id: AcpTerminalOwnerId) -> Result<(), RuntimeError> {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        while state
            .owners
            .get(&owner_id)
            .is_some_and(|owner| owner.cleanup_in_progress)
        {
            state = self
                .inner
                .changed
                .wait(state)
                .expect("terminal registry poisoned");
        }
        let owner = state
            .owners
            .get_mut(&owner_id)
            .ok_or_else(|| RuntimeError::NotReady("ACP terminal owner is missing".to_string()))?;
        if owner.phase == TerminalOwnerPhase::Closed {
            return Err(RuntimeError::NotReady(
                "ACP terminal session is closed".to_string(),
            ));
        }
        owner.phase = TerminalOwnerPhase::Active;
        Ok(())
    }

    fn cleanup(
        &self,
        owner_id: AcpTerminalOwnerId,
        phase: TerminalOwnerPhase,
    ) -> Result<(), RuntimeError> {
        let terminals = {
            let mut state = self.inner.state.lock().expect("terminal registry poisoned");
            loop {
                let cleanup_in_progress = {
                    let owner = state.owners.get_mut(&owner_id).ok_or_else(|| {
                        RuntimeError::NotReady("ACP terminal owner is missing".to_string())
                    })?;
                    owner.phase = closed_phase(owner.phase, phase);
                    if owner.cleanup_in_progress {
                        true
                    } else {
                        owner.cleanup_in_progress = true;
                        false
                    }
                };
                if cleanup_in_progress {
                    state = self
                        .inner
                        .changed
                        .wait(state)
                        .expect("terminal registry poisoned");
                    continue;
                }
                while state
                    .owners
                    .get(&owner_id)
                    .expect("terminal owner disappeared")
                    .in_flight_creates
                    > 0
                {
                    state = self
                        .inner
                        .changed
                        .wait(state)
                        .expect("terminal registry poisoned");
                }
                if state.current_open == Some(owner_id) {
                    state.current_open = None;
                }
                break std::mem::take(
                    &mut state
                        .owners
                        .get_mut(&owner_id)
                        .expect("terminal owner disappeared")
                        .terminals,
                );
            }
        };

        let (failed, error) = self.cleanup_terminals(terminals);
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        let owner = state
            .owners
            .get_mut(&owner_id)
            .expect("terminal owner disappeared");
        owner.terminals.extend(failed);
        owner.cleanup_in_progress = false;
        self.inner.changed.notify_all();
        error.map_or(Ok(()), Err)
    }

    fn cleanup_terminals(
        &self,
        terminals: HashSet<OwnedTerminal>,
    ) -> (Vec<OwnedTerminal>, Option<RuntimeError>) {
        let killed = terminals
            .iter()
            .filter(|terminal| {
                kill_host_terminal(
                    &self.inner.host_bridge,
                    &terminal.session_id,
                    &terminal.terminal_id,
                )
            })
            .cloned()
            .collect::<Vec<_>>();
        for terminal in killed {
            wait_for_host_terminal_exit(
                &self.inner.host_bridge,
                &terminal.session_id,
                &terminal.terminal_id,
            );
        }
        let mut failed = Vec::new();
        let mut first_error = None;
        for terminal in terminals {
            if let Err(error) = release_host_terminal(
                &self.inner.host_bridge,
                &terminal.session_id,
                &terminal.terminal_id,
            ) {
                first_error.get_or_insert(error);
                failed.push(terminal);
            }
        }
        (failed, first_error)
    }

    fn finish_create(&self, owner_id: AcpTerminalOwnerId, terminal: Option<OwnedTerminal>) {
        let mut state = self.inner.state.lock().expect("terminal registry poisoned");
        let owner = state
            .owners
            .get_mut(&owner_id)
            .expect("terminal owner disappeared");
        if let Some(terminal) = terminal {
            owner.terminals.insert(terminal);
        }
        owner.in_flight_creates = owner
            .in_flight_creates
            .checked_sub(1)
            .expect("terminal create permit finished twice");
        self.inner.changed.notify_all();
    }
}

impl AcpTerminalOwner {
    pub(super) fn activate_session(&self, session_id: &str) {
        self.registry.activate_session(self.id, session_id);
    }

    pub(super) fn activate(&self) -> Result<(), RuntimeError> {
        self.registry.activate(self.id)
    }

    pub(super) fn cancel(&self) -> Result<(), RuntimeError> {
        self.registry
            .cleanup(self.id, TerminalOwnerPhase::Cancelled)
    }

    pub(super) fn close(&self) -> Result<(), RuntimeError> {
        self.registry.cleanup(self.id, TerminalOwnerPhase::Closed)
    }
}

impl AcpTerminalCreatePermit {
    pub(super) fn complete(mut self, terminal_id: &str) {
        self.completed = true;
        self.registry.finish_create(
            self.owner_id,
            Some(OwnedTerminal {
                session_id: self.session_id.clone(),
                terminal_id: terminal_id.to_string(),
            }),
        );
    }
}

impl Drop for AcpTerminalCreatePermit {
    fn drop(&mut self) {
        if !self.completed {
            self.registry.finish_create(self.owner_id, None);
        }
    }
}

fn closed_phase(current: TerminalOwnerPhase, requested: TerminalOwnerPhase) -> TerminalOwnerPhase {
    if current == TerminalOwnerPhase::Closed || requested == TerminalOwnerPhase::Closed {
        TerminalOwnerPhase::Closed
    } else {
        requested
    }
}
