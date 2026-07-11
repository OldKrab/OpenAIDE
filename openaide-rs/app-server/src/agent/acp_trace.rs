use serde::Serialize;

mod file;
mod naming;
mod session;
mod state;

pub use session::AcpTraceSession;
pub use state::AcpTraceState;

#[derive(Debug, Clone, Serialize)]
pub struct AcpTraceStatus {
    pub enabled: bool,
    pub directory: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeSettings {
    pub developer: RuntimeDeveloperSettings,
}

#[derive(Debug, Clone, Serialize)]
pub struct RuntimeDeveloperSettings {
    pub acp_trace: AcpTraceStatus,
}

#[cfg(test)]
mod tests;
