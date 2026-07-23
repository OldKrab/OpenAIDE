use std::collections::HashSet;

use crate::agent::gateway::AgentGateway;
use crate::agent::{AgentSession, AgentSessionSetConfigOptionRequest};
use crate::protocol::model::{
    ConfigOption, ConfigOptionCategory, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionsCatalog,
};
use crate::storage::agent_config_preferences::{AgentConfigPreference, AgentConfigPreferences};
use crate::storage::Store;

const MAX_RECONCILIATION_PASSES: usize = 2;

/// Applies remembered user choices only after ACP has supplied the new session's
/// authoritative catalog. Every response replaces the catalog before continuing.
pub(crate) fn apply_to_prepared_session(
    store: &Store,
    agent_gateway: &AgentGateway,
    mut session: AgentSession,
) -> AgentSession {
    let preferences = match store.read_agent_config_preferences(&session.agent_id) {
        Ok(preferences) => preferences,
        Err(error) => {
            crate::logging::warn(
                "agent_config_preferences_read_failed",
                serde_json::json!({
                    "agent_id": session.agent_id,
                    "error": error.to_string(),
                }),
            );
            return session;
        }
    };
    if preferences.options.is_empty() || session.config_catalog.is_none() {
        return session;
    }

    let mut failed_options = HashSet::new();
    for _ in 0..MAX_RECONCILIATION_PASSES {
        let before = session.config_catalog.clone();
        apply_pass(
            agent_gateway,
            &preferences,
            &mut session,
            &mut failed_options,
        );
        if session.config_catalog == before || preferences_match_session(&preferences, &session) {
            break;
        }
    }
    if !preferences_match_session(&preferences, &session) {
        crate::logging::warn(
            "agent_config_preferences_not_settled",
            serde_json::json!({
                "agent_id": session.agent_id,
                "session_id": session.session_id,
            }),
        );
    }
    session
}

fn apply_pass(
    agent_gateway: &AgentGateway,
    preferences: &AgentConfigPreferences,
    session: &mut AgentSession,
    failed_options: &mut HashSet<String>,
) {
    let option_ids = ordered_option_ids(session.config_catalog.as_ref());
    for option_id in option_ids {
        if failed_options.contains(&option_id) {
            continue;
        }
        let Some(preference) = preferences
            .options
            .iter()
            .find(|preference| preference.id == option_id)
        else {
            continue;
        };
        let Some(option) = current_option(session, &option_id) else {
            continue;
        };
        if !supports_preference(option, preference) || option.current_value == preference.value {
            continue;
        }
        let request = AgentSessionSetConfigOptionRequest {
            agent_id: session.agent_id.clone(),
            session_id: session.session_id.clone(),
            config_id: option_id.clone(),
            value: preference.value.clone(),
        };
        match agent_gateway.set_session_config_option(request) {
            Ok(catalog) => *session = session.clone().with_config_options(&catalog),
            Err(error) => {
                failed_options.insert(option_id.clone());
                crate::logging::warn(
                    "agent_config_preference_apply_failed",
                    serde_json::json!({
                        "agent_id": session.agent_id,
                        "session_id": session.session_id,
                        "config_id": option_id,
                        "error": error.to_string(),
                    }),
                );
            }
        }
    }
}

fn ordered_option_ids(catalog: Option<&ConfigOptionsCatalog>) -> Vec<String> {
    let Some(catalog) = catalog else {
        return Vec::new();
    };
    let mut options = catalog.options.iter().enumerate().collect::<Vec<_>>();
    options.sort_by_key(|(index, option)| (category_priority(option.category.as_ref()), *index));
    options
        .into_iter()
        .map(|(_, option)| option.id.clone())
        .collect()
}

fn category_priority(category: Option<&ConfigOptionCategory>) -> u8 {
    match category {
        Some(ConfigOptionCategory::Mode) => 0,
        Some(ConfigOptionCategory::Model) => 1,
        Some(ConfigOptionCategory::ThoughtLevel) => 2,
        Some(ConfigOptionCategory::Other) | None => 3,
    }
}

fn current_option<'a>(session: &'a AgentSession, option_id: &str) -> Option<&'a ConfigOption> {
    session
        .config_catalog
        .as_ref()?
        .options
        .iter()
        .find(|option| option.id == option_id)
}

fn supports_preference(option: &ConfigOption, preference: &AgentConfigPreference) -> bool {
    match (&option.kind, &preference.value) {
        (ConfigOptionKind::Select, ConfigOptionCurrentValue::Id { value }) => {
            option.values.iter().any(|candidate| candidate.id == *value)
        }
        (ConfigOptionKind::Boolean, ConfigOptionCurrentValue::Boolean { .. }) => true,
        _ => false,
    }
}

fn preferences_match_session(preferences: &AgentConfigPreferences, session: &AgentSession) -> bool {
    preferences.options.iter().all(|preference| {
        current_option(session, &preference.id)
            .filter(|option| supports_preference(option, preference))
            .is_none_or(|option| option.current_value == preference.value)
    })
}
