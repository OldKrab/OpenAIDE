use std::sync::Arc;

use super::AcpAgentProcessPool;
use crate::agent::managed_acp_provisioner::{
    ManagedAcpInstaller, ManagedAcpProvisioner, ManagedAcpRuntime,
};
use crate::agent::registry::AgentRegistry;
use crate::agent::registry_handle::AgentRegistryHandle;
use crate::protocol::host::HostBridge;

struct UnusedInstaller;

impl ManagedAcpInstaller for UnusedInstaller {
    fn install(&self, _: ManagedAcpRuntime, _: &std::path::Path) -> Result<(), String> {
        panic!("passive discovery must not install a managed runtime")
    }
}

#[test]
fn passive_discovery_waits_for_every_managed_runtime_to_be_installed() {
    let storage = tempfile::tempdir().unwrap();
    let mut pool = AcpAgentProcessPool::new(
        AgentRegistryHandle::new(AgentRegistry::default_built_ins()),
        HostBridge::default(),
    );
    pool.with_managed_provisioner(ManagedAcpProvisioner::with_installer(
        storage.path().to_path_buf(),
        Arc::new(UnusedInstaller),
    ));

    assert!(!pool.allows_passive_session_discovery("codex"));
    assert!(!pool.allows_passive_session_discovery("claude-code"));
    assert!(pool.allows_passive_session_discovery("opencode"));
}
