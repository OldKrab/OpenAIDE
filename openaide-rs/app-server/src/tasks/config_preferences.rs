use std::collections::HashSet;
use std::time::Instant;

use crate::agent::gateway::AgentGateway;
use crate::agent::{AgentSession, AgentSessionSetConfigOptionRequest};
use crate::protocol::model::{
    ConfigOption, ConfigOptionCategory, ConfigOptionCurrentValue, ConfigOptionKind,
    ConfigOptionsCatalog,
};
use crate::storage::agent_config_preferences::{AgentConfigPreference, AgentConfigPreferences};
use crate::storage::records::TaskRecord;
use crate::storage::Store;

const MAX_RECONCILIATION_PASSES: usize = 2;

/// Applies remembered user choices only after ACP has supplied the new session's
/// authoritative catalog. Every response replaces the catalog before continuing.
pub(crate) fn apply_to_prepared_session(
    store: &Store,
    agent_gateway: &AgentGateway,
    mut session: AgentSession,
) -> Result<AgentSession, crate::protocol::errors::RuntimeError> {
    let preferences = store.read_agent_config_preferences(&session.agent_id)?;
    if preferences.options.is_empty() || session.config_catalog.is_none() {
        return Ok(session);
    }

    let failed_options = reconcile(agent_gateway, &preferences.options, &mut session);
    if !failed_options.is_empty() || !preferences_match_session(&preferences.options, &session) {
        crate::logging::warn(
            "agent_config_preferences_not_settled",
            serde_json::json!({
                "agent_id": session.agent_id,
                "session_id": session.session_id,
            }),
        );
        return Err(crate::protocol::errors::RuntimeError::NotReady(
            "Couldn’t apply your preferences.".into(),
        ));
    }
    Ok(session)
}

/// Puts a restored session back to the values its Task last chose, when the Agent restored
/// it with others, often its defaults. Best effort: a value the catalog no longer offers is
/// skipped, a rejected one is logged, and the session keeps whatever the Agent last
/// confirmed. Its catalog stays authoritative, so the controls show the actual values.
pub(crate) fn restore_chosen(
    agent_gateway: &AgentGateway,
    task: &TaskRecord,
    mut session: AgentSession,
) -> AgentSession {
    let chosen = &task.config_mutation.chosen;
    if chosen.is_empty() || session.config_catalog.is_none() {
        return session;
    }
    if preferences_match_session(chosen, &session) {
        return session;
    }
    let started = Instant::now();
    crate::logging::info(
        "task_config_restore_started",
        serde_json::json!({ "task_id": task.task_id, "agent_id": session.agent_id }),
    );
    let failed_options = reconcile(agent_gateway, chosen, &mut session);
    let settled = preferences_match_session(chosen, &session);
    let fields = serde_json::json!({
        "task_id": task.task_id,
        "agent_id": session.agent_id,
        "outcome": if settled { "settled" } else { "unsettled" },
        "failed_count": failed_options.len(),
        "duration_ms": started.elapsed().as_millis(),
    });
    if settled {
        crate::logging::info("task_config_restore_completed", fields);
    } else {
        crate::logging::warn("task_config_restore_completed", fields);
    }
    session
}

/// Applies [`preferences`] in catalog order with one bounded second pass, for values a
/// preceding option reset. Returns the options the Agent rejected.
fn reconcile(
    agent_gateway: &AgentGateway,
    preferences: &[AgentConfigPreference],
    session: &mut AgentSession,
) -> HashSet<String> {
    let mut failed_options = HashSet::new();
    for _ in 0..MAX_RECONCILIATION_PASSES {
        let before = session.config_catalog.clone();
        apply_pass(agent_gateway, preferences, session, &mut failed_options);
        if session.config_catalog == before || preferences_match_session(preferences, session) {
            break;
        }
    }
    failed_options
}

fn apply_pass(
    agent_gateway: &AgentGateway,
    preferences: &[AgentConfigPreference],
    session: &mut AgentSession,
    failed_options: &mut HashSet<String>,
) {
    let option_ids = ordered_option_ids(session.config_catalog.as_ref());
    for option_id in option_ids {
        if failed_options.contains(&option_id) {
            continue;
        }
        let Some(preference) = preferences
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
            diagnostic_operation_id: None,
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
                        "error_code": error.code(),
                    }),
                );
            }
        }
    }
}

pub(crate) fn ordered_option_ids(catalog: Option<&ConfigOptionsCatalog>) -> Vec<String> {
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

pub(crate) fn supports_preference(
    option: &ConfigOption,
    preference: &AgentConfigPreference,
) -> bool {
    match (&option.kind, &preference.value) {
        (ConfigOptionKind::Select, ConfigOptionCurrentValue::Id { value }) => {
            option.values.iter().any(|candidate| candidate.id == *value)
        }
        (ConfigOptionKind::Boolean, ConfigOptionCurrentValue::Boolean { .. }) => true,
        _ => false,
    }
}

fn preferences_match_session(
    preferences: &[AgentConfigPreference],
    session: &AgentSession,
) -> bool {
    preferences.iter().all(|preference| {
        current_option(session, &preference.id)
            .filter(|option| supports_preference(option, preference))
            .is_none_or(|option| option.current_value == preference.value)
    })
}

/// Computes only from the current Agent catalog; matching values require no mutation.
pub(crate) fn initial_state(
    preferences: &AgentConfigPreferences,
    catalog: Option<&ConfigOptionsCatalog>,
) -> Option<openaide_app_server_protocol::snapshot::AgentConfigPreferencesSnapshot> {
    use openaide_app_server_protocol::snapshot::{
        AgentConfigPreferencesSnapshot, AgentConfigPreferencesState,
    };
    let mut differs = false;
    let mut skipped_count = 0;
    for preference in &preferences.options {
        match catalog.and_then(|catalog| {
            catalog
                .options
                .iter()
                .find(|option| option.id == preference.id)
        }) {
            Some(option) if supports_preference(option, preference) => {
                differs |= option.current_value != preference.value
            }
            _ => skipped_count += 1,
        }
    }
    (differs || skipped_count > 0).then_some(AgentConfigPreferencesSnapshot {
        state: if differs {
            AgentConfigPreferencesState::Applying
        } else {
            AgentConfigPreferencesState::Settled
        },
        skipped_count,
    })
}
