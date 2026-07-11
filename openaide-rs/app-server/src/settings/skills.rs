use openaide_app_server_protocol::errors::ProtocolError;
use openaide_app_server_protocol::settings::{
    SettingsProjectionAvailability, SettingsSkillsParams, SettingsSkillsResult,
};

use crate::time::now_string;

pub(crate) trait SkillsSettingsWorkflow: Send + Sync {
    fn skills_settings(
        &self,
        params: SettingsSkillsParams,
    ) -> Result<SettingsSkillsResult, ProtocolError>;
}

#[derive(Debug, Clone, Default)]
pub(crate) struct SkillsSettingsService;

impl SkillsSettingsService {
    pub(crate) fn new() -> Self {
        Self
    }
}

impl SkillsSettingsWorkflow for SkillsSettingsService {
    fn skills_settings(
        &self,
        _params: SettingsSkillsParams,
    ) -> Result<SettingsSkillsResult, ProtocolError> {
        Ok(SettingsSkillsResult {
            generated_at: now_string(),
            availability: SettingsProjectionAvailability::Unavailable,
            skills: Vec::new(),
            notices: Vec::new(),
        })
    }
}
