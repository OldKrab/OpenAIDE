use std::sync::Arc;

#[cfg(test)]
use crate::agent::acp_schema::SessionNotification;
use crate::agent::acp_schema::{MaybeUndefined, SessionUpdate};
#[cfg(test)]
use agent_client_protocol::util::MatchDispatch;

#[cfg(test)]
use crate::agent::acp_errors::acp_error;
use crate::agent::acp_update_projection::{normalize_available_commands, normalize_config_options};
use crate::agent::{
    AgentMetadataField, AgentSession, AgentSessionEventSink, AgentSessionMetadataUpdate,
};
use crate::protocol::errors::RuntimeError;
use crate::protocol::model::{AgentCommandsCatalog, ConfigOptionsCatalog};

#[derive(Default)]
pub(super) struct PendingSessionCatalogs {
    config: Option<ConfigOptionsCatalog>,
    commands: Option<AgentCommandsCatalog>,
    metadata: Option<AgentSessionMetadataUpdate>,
}

#[derive(Clone, Default)]
pub(super) struct DispatchSessionCatalogs {
    pub(super) config: Option<ConfigOptionsCatalog>,
    pub(super) commands: Option<AgentCommandsCatalog>,
    pub(super) metadata: Option<AgentSessionMetadataUpdate>,
}

#[cfg(test)]
pub(super) async fn session_catalogs_from_dispatch(
    agent_id: &str,
    dispatch: agent_client_protocol::Dispatch,
) -> Result<DispatchSessionCatalogs, RuntimeError> {
    let catalogs = Arc::new(std::sync::Mutex::new(DispatchSessionCatalogs::default()));
    let catalogs_sink = catalogs.clone();
    MatchDispatch::new(dispatch)
        .if_notification(async move |notification: SessionNotification| {
            *catalogs_sink
                .lock()
                .expect("ACP session catalog update lock poisoned") =
                session_catalogs_from_update(agent_id, &notification.update);
            Ok(())
        })
        .await
        .otherwise_ignore()
        .map_err(acp_error)?;
    let result = std::mem::take(
        &mut *catalogs
            .lock()
            .expect("ACP session catalog update lock poisoned"),
    );
    Ok(result)
}

pub(super) fn session_catalogs_from_update(
    agent_id: &str,
    update: &SessionUpdate,
) -> DispatchSessionCatalogs {
    let mut catalogs = DispatchSessionCatalogs::default();
    match update {
        SessionUpdate::ConfigOptionUpdate(update) => {
            catalogs.config = Some(normalize_config_options(
                agent_id,
                update.config_options.clone(),
            ));
        }
        SessionUpdate::AvailableCommandsUpdate(update) => {
            catalogs.commands = Some(normalize_available_commands(update.clone()));
        }
        SessionUpdate::SessionInfoUpdate(update) => {
            catalogs.metadata = Some(metadata_update_from_acp(update.clone()));
        }
        _ => {}
    }
    catalogs
}

fn metadata_update_from_acp(
    update: crate::agent::acp_schema::SessionInfoUpdate,
) -> AgentSessionMetadataUpdate {
    AgentSessionMetadataUpdate {
        title: metadata_field(update.title),
        updated_at: metadata_field(update.updated_at),
    }
}

fn metadata_field(value: MaybeUndefined<String>) -> AgentMetadataField<String> {
    match value {
        MaybeUndefined::Undefined => AgentMetadataField::Unchanged,
        MaybeUndefined::Null => AgentMetadataField::Clear,
        MaybeUndefined::Value(value) => AgentMetadataField::Value(value),
    }
}

#[cfg(test)]
pub(super) fn attach_session_event_sink_to_slot(
    session_event_sink: &mut Option<Arc<dyn AgentSessionEventSink>>,
    pending_catalogs: &mut PendingSessionCatalogs,
    sink: Arc<dyn AgentSessionEventSink>,
) -> Result<(), RuntimeError> {
    attach_session_event_sink_with_catalog_snapshot(
        session_event_sink,
        pending_catalogs,
        None,
        None,
        sink,
    )
}

/// Installs a session sink and gives it the attachment's latest catalog snapshots.
///
/// The worker serializes commands and ACP updates. Its snapshot therefore supersedes
/// buffered catalog updates that predate this attachment. Metadata has no equivalent
/// snapshot and is replayed normally.
pub(super) fn attach_session_event_sink_with_catalog_snapshot(
    session_event_sink: &mut Option<Arc<dyn AgentSessionEventSink>>,
    pending_catalogs: &mut PendingSessionCatalogs,
    config_snapshot: Option<&ConfigOptionsCatalog>,
    commands_snapshot: Option<&AgentCommandsCatalog>,
    sink: Arc<dyn AgentSessionEventSink>,
) -> Result<(), RuntimeError> {
    *session_event_sink = Some(sink.clone());
    if let Some(catalog) = config_snapshot {
        pending_catalogs.config = None;
        sink.config_options_changed(catalog.clone())?;
    } else if let Some(catalog) = pending_catalogs.config.take() {
        sink.config_options_changed(catalog)?;
    }
    if let Some(catalog) = commands_snapshot {
        pending_catalogs.commands = None;
        sink.commands_changed(catalog.clone())?;
    } else if let Some(catalog) = pending_catalogs.commands.take() {
        sink.commands_changed(catalog)?;
    }
    if let Some(update) = pending_catalogs.metadata.take() {
        sink.metadata_changed(update)?;
    }
    Ok(())
}

/// Applies live catalogs without turning an absent config catalog into a false empty catalog.
pub(super) fn session_with_catalog_snapshots(
    session: &AgentSession,
    config_catalog: &ConfigOptionsCatalog,
    commands_catalog: &Option<AgentCommandsCatalog>,
) -> AgentSession {
    let session = session
        .clone()
        .with_commands_catalog(commands_catalog.clone());
    if config_catalog.agent_id.is_empty() {
        session
    } else {
        session.with_config_options(config_catalog)
    }
}

pub(super) fn deliver_session_metadata_update(
    update: AgentSessionMetadataUpdate,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    pending_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    if let Some(sink) = session_event_sink {
        sink.metadata_changed(update)?;
    } else if let Some(pending) = pending_catalogs.metadata.as_mut() {
        merge_metadata_field(&mut pending.title, update.title);
        merge_metadata_field(&mut pending.updated_at, update.updated_at);
    } else {
        pending_catalogs.metadata = Some(update);
    }
    Ok(())
}

fn merge_metadata_field<T>(current: &mut AgentMetadataField<T>, update: AgentMetadataField<T>) {
    if !matches!(update, AgentMetadataField::Unchanged) {
        *current = update;
    }
}

pub(super) fn deliver_session_config_catalog(
    catalog: ConfigOptionsCatalog,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    pending_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    if let Some(sink) = session_event_sink {
        sink.config_options_changed(catalog)?;
    } else {
        pending_catalogs.config = Some(catalog);
    }
    Ok(())
}

pub(super) fn deliver_session_commands_catalog(
    catalog: AgentCommandsCatalog,
    session_event_sink: Option<&Arc<dyn AgentSessionEventSink>>,
    pending_catalogs: &mut PendingSessionCatalogs,
) -> Result<(), RuntimeError> {
    if let Some(sink) = session_event_sink {
        sink.commands_changed(catalog)?;
    } else {
        pending_catalogs.commands = Some(catalog);
    }
    Ok(())
}
