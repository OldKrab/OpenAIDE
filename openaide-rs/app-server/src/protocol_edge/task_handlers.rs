use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{
    TaskAdoptNativeSessionParams, TaskAdoptNativeSessionResult, TaskCancelParams, TaskCancelResult,
    TaskChatPageParams, TaskChatPageResult, TaskCreateParams, TaskCreateResult, TaskDiscardParams,
    TaskDiscardResult, TaskListParams, TaskListResult, TaskMarkReadParams, TaskMarkReadResult,
    TaskOpenParams, TaskOpenResult, TaskRetryHistorySyncParams, TaskRetryHistorySyncResult,
    TaskSendParams, TaskSendResult, TaskSetArchivedParams, TaskSetArchivedResult,
    TaskSetConfigOptionParams, TaskSetConfigOptionResult, TaskToolDetailParams,
    TaskToolDetailResult,
};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayEventDelivery, GatewayOutcome, GatewayResponse, RpcGateway};

mod publication;

impl RpcGateway {
    pub(super) fn handle_task_list(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskListParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let result = match self.task_snapshots.list(
            params.archived,
            params.project_id.as_ref(),
            params.cursor.as_ref(),
        ) {
            Ok(snapshot) => TaskListResult {
                tasks: snapshot.tasks,
                revision: snapshot.revision,
                next_cursor: snapshot.next_cursor,
            },
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskListResult>(connection_id, id, meta, result)
    }

    pub(super) fn handle_task_create(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskCreateParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_create.create(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskCreateResult>(connection_id, id, meta, TaskCreateResult { task })
    }

    pub(super) fn handle_task_adopt_native_session(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskAdoptNativeSessionParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_adopt_native_session.adopt_native_session(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskAdoptNativeSessionResult>(
            connection_id,
            id,
            meta,
            TaskAdoptNativeSessionResult { task },
        )
    }

    pub(super) fn handle_task_send(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSendParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task send");
        let accepted = match self
            .task_send
            .send_for_client(&client.client_instance_id, params)
        {
            Ok(accepted) => accepted,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSendResult>(
            connection_id,
            id,
            meta,
            TaskSendResult {
                task: accepted.task,
                turn_id: accepted.turn_id,
                user_message_id: accepted.user_message_id,
            },
        )
    }

    pub(super) fn handle_task_cancel(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskCancelParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_cancel.cancel(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskCancelResult>(connection_id, id, meta, TaskCancelResult { task })
    }

    pub(super) fn handle_task_set_config_option(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSetConfigOptionParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_set_config_option.set_config_option(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSetConfigOptionResult>(
            connection_id,
            id,
            meta,
            TaskSetConfigOptionResult { task },
        )
    }

    pub(super) fn handle_task_open(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskOpenParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_open.open(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let task = self.task_with_pending_requests(task);
        self.result::<TaskOpenResult>(connection_id, id, meta, TaskOpenResult { task })
    }

    pub(super) fn handle_task_retry_history_sync(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskRetryHistorySyncParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_open.retry_history_sync(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let task = self.task_with_pending_requests(task);
        self.result::<TaskRetryHistorySyncResult>(
            connection_id,
            id,
            meta,
            TaskRetryHistorySyncResult { task },
        )
    }

    pub(super) fn handle_task_mark_read(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskMarkReadParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task = match self.task_open.mark_read(params) {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let task = self.task_with_pending_requests(task);
        self.result::<TaskMarkReadResult>(connection_id, id, meta, TaskMarkReadResult { task })
    }

    pub(super) fn handle_task_chat_page(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskChatPageParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let page = match self.task_chat_page.chat_page(params) {
            Ok(page) => page,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskChatPageResult>(connection_id, id, meta, page)
    }

    pub(super) fn handle_task_tool_detail(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskToolDetailParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let details = match self.task_tool_detail.tool_detail(params) {
            Ok(details) => details,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskToolDetailResult>(connection_id, id, meta, details)
    }

    pub(super) fn handle_task_discard(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskDiscardParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let discarded_task_id = params.task_id.clone();
        let tasks = match self.task_discard.discard(params) {
            Ok(tasks) => tasks,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskDiscardResult>(
            connection_id,
            id,
            meta,
            TaskDiscardResult {
                discarded_task_id,
                tasks,
            },
        )
    }

    pub(super) fn handle_task_set_archived(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSetArchivedParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task_id = params.task_id.clone();
        let archived = params.archived;
        let tasks = match self.task_archive.set_archived(params) {
            Ok(tasks) => tasks,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSetArchivedResult>(
            connection_id,
            id,
            meta,
            TaskSetArchivedResult {
                task_id,
                archived,
                tasks,
            },
        )
    }

    pub(super) fn result_with_events<T: serde::Serialize>(
        &self,
        connection_id: ConnectionId,
        id: String,
        meta: RequestMeta,
        result: T,
        events: Vec<GatewayEventDelivery>,
    ) -> GatewayOutcome {
        GatewayOutcome::Respond {
            connection_id,
            id,
            response: GatewayResponse::Result(
                serde_json::to_value(
                    openaide_app_server_protocol::envelopes::ResponseEnvelope::new(
                        result,
                        openaide_app_server_protocol::envelopes::ResponseMeta {
                            client_request_id: meta.client_request_id,
                        },
                    ),
                )
                .expect("protocol response should serialize"),
            ),
            events,
            server_requests: Vec::new(),
        }
    }
}
