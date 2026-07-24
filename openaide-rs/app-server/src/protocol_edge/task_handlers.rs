use openaide_app_server_protocol::envelopes::RequestMeta;
use openaide_app_server_protocol::task::{
    TaskAcquireInWorktreeParams, TaskAcquireInWorktreeResult, TaskAcquireParams, TaskAcquireResult,
    TaskAdoptNativeSessionParams, TaskAdoptNativeSessionResult, TaskArchiveParams,
    TaskArchiveResult, TaskCancelParams, TaskCancelResult, TaskChatPageParams, TaskChatPageResult,
    TaskListParams, TaskListResult, TaskMarkReadParams, TaskMarkReadResult, TaskOpenParams,
    TaskOpenResult, TaskReleaseParams, TaskReleaseResult, TaskRestoreParams, TaskRestoreResult,
    TaskSearchFilesParams, TaskSearchFilesResult, TaskSendParams, TaskSendResult,
    TaskSetConfigOptionParams, TaskSetConfigOptionResult, TaskSetTitleParams, TaskSetTitleResult,
};
use serde_json::Value;

use crate::client_lifecycle::{AppServerTime, ConnectionId};

use super::{responses, GatewayEventDelivery, GatewayOutcome, GatewayResponse, RpcGateway};

mod publication;

impl RpcGateway {
    pub(super) fn handle_task_acquire_in_worktree(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskAcquireInWorktreeParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task acquire");
        match self
            .task_acquire
            .acquire_in_worktree_for_client(&client.client_instance_id, params)
        {
            Ok(task) => self.result::<TaskAcquireInWorktreeResult>(
                connection_id,
                id,
                meta,
                TaskAcquireInWorktreeResult { task },
            ),
            Err(error) => self.error(connection_id, id, meta, error),
        }
    }

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
            params.lifecycle,
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

    pub(super) fn handle_task_acquire(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskAcquireParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task acquire");
        let task = match self
            .task_acquire
            .acquire_for_client(&client.client_instance_id, params)
        {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskAcquireResult>(connection_id, id, meta, TaskAcquireResult { task })
    }

    pub(super) fn handle_task_search_files(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSearchFilesParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for file search");
        let result = match self
            .task_file_search
            .search_files_for_client(&client.client_instance_id, params)
        {
            Ok(result) => result,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSearchFilesResult>(connection_id, id, meta, result)
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
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task cancel");
        let task = match self
            .task_cancel
            .cancel_for_client(&client.client_instance_id, params)
        {
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
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for config changes");
        let task = match self
            .task_set_config_option
            .set_config_option_for_client(&client.client_instance_id, params)
        {
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

    pub(super) fn handle_task_set_title(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskSetTitleParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for Task title changes");
        let task = match self
            .task_set_title
            .set_title_for_client(&client.client_instance_id, params)
        {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskSetTitleResult>(connection_id, id, meta, TaskSetTitleResult { task })
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
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task open");
        let task = match self
            .task_open
            .open_for_client(&client.client_instance_id, params)
        {
            Ok(task) => task,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        let task = self.task_with_pending_requests(task);
        self.result::<TaskOpenResult>(connection_id, id, meta, TaskOpenResult { task })
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
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task mark-read");
        let task = match self
            .task_open
            .mark_read_for_client(&client.client_instance_id, params)
        {
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
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for Chat paging");
        let page = match self
            .task_chat_page
            .chat_page_for_client(&client.client_instance_id, params)
        {
            Ok(page) => page,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskChatPageResult>(connection_id, id, meta, page)
    }

    pub(super) fn handle_task_release(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskReleaseParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let task_id = params.task_id.clone();
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task release");
        if let Err(error) = self
            .task_release
            .release_for_client(&client.client_instance_id, params)
        {
            return self.error(connection_id, id, meta, error);
        }
        let scope = openaide_app_server_protocol::state::SubscriptionScope::Task {
            task_id: task_id.clone(),
        };
        self.state_stream.unsubscribe(&client, scope, now);
        self.server_requests.observe_subscription_removed(
            &client.client_instance_id,
            &task_id,
            now,
        );
        self.result::<TaskReleaseResult>(connection_id, id, meta, TaskReleaseResult { task_id })
    }

    pub(super) fn handle_task_archive(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
        _now: AppServerTime,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskArchiveParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task archive");
        let change = match self
            .task_archive
            .archive_for_client(&client.client_instance_id, params)
        {
            Ok(change) => change,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskArchiveResult>(connection_id, id, meta, TaskArchiveResult { change })
    }

    pub(super) fn handle_task_restore(
        &mut self,
        connection_id: ConnectionId,
        id: String,
        params: Value,
        meta: RequestMeta,
    ) -> GatewayOutcome {
        let params = match serde_json::from_value::<TaskRestoreParams>(params) {
            Ok(params) => params,
            Err(error) => {
                return self.error(connection_id, id, meta, responses::invalid_params(error))
            }
        };
        let client = self
            .client_hub
            .context_for_connection(&connection_id)
            .expect("routing requires an initialized client for task restore");
        let change = match self
            .task_archive
            .restore_for_client(&client.client_instance_id, params)
        {
            Ok(change) => change,
            Err(error) => return self.error(connection_id, id, meta, error),
        };
        self.result::<TaskRestoreResult>(connection_id, id, meta, TaskRestoreResult { change })
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
