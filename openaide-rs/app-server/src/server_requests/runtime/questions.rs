use openaide_app_server_protocol::ids::{RequestId, TaskId};
use openaide_app_server_protocol::server_requests::{
    QuestionRequestParams, QuestionRequestResponse, QUESTION_REQUEST,
};
use openaide_app_server_protocol::snapshot::PendingRequestScope;

use crate::agent::TurnCancellation;
use crate::client_lifecycle::AppServerTime;
use crate::protocol::errors::RuntimeError;
use crate::server_requests::{OpenRequestOutcome, ServerRequestDraft};

use super::{ServerRequestRuntime, ServerRequestRuntimeInner};

pub(super) struct QuestionWaiter {
    pub(super) form: QuestionRequestParams,
    pub(super) result: Option<QuestionRequestResponse>,
}

impl ServerRequestRuntime {
    pub fn open_question_request(
        &self,
        task_id: &str,
        form: QuestionRequestParams,
    ) -> Result<Option<RequestId>, RuntimeError> {
        let params = serde_json::to_value(&form)
            .map_err(|error| RuntimeError::Internal(error.to_string()))?;
        let draft = ServerRequestDraft {
            scope: PendingRequestScope::Task {
                task_id: TaskId::from(task_id.to_string()),
            },
            method: QUESTION_REQUEST.to_string(),
            title: "Question".to_string(),
            params,
        };
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        let opened = inner.broker.open(draft, Vec::new(), AppServerTime(0));
        let OpenRequestOutcome::Opened {
            snapshot,
            deliveries,
        } = opened
        else {
            return Ok(None);
        };
        inner.broker.defer_deliveries(&deliveries);
        inner.question_waiters.insert(
            snapshot.request_id.clone(),
            QuestionWaiter { form, result: None },
        );
        Ok(Some(snapshot.request_id))
    }

    pub fn wait_question_response(
        &self,
        request_id: &RequestId,
        cancellation: &TurnCancellation,
    ) -> Result<QuestionRequestResponse, RuntimeError> {
        let mut inner = self.inner.lock().expect("server request runtime poisoned");
        loop {
            if cancellation.is_cancelled() {
                inner.broker.interrupt_request(request_id, AppServerTime(0));
                inner.question_waiters.remove(request_id);
                return Ok(QuestionRequestResponse::Cancel);
            }
            let Some(waiter) = inner.question_waiters.get_mut(request_id) else {
                return Ok(QuestionRequestResponse::Cancel);
            };
            if let Some(result) = waiter.result.take() {
                inner.question_waiters.remove(request_id);
                return Ok(result);
            }
            let (next, _) = self
                .changed
                .wait_timeout(inner, std::time::Duration::from_millis(50))
                .expect("server request runtime poisoned");
            inner = next;
        }
    }
}

pub(super) fn normalize_question_answer(
    inner: &ServerRequestRuntimeInner,
    request_id: &RequestId,
    answer: crate::server_requests::ServerRequestAnswer,
) -> crate::server_requests::ServerRequestAnswer {
    let Some(waiter) = inner.question_waiters.get(request_id) else {
        return answer;
    };
    let crate::server_requests::ServerRequestAnswer::Result(value) = answer else {
        return answer;
    };
    match serde_json::from_value::<QuestionRequestResponse>(value.clone()) {
        Ok(response)
            if crate::agent::acp_elicitation_form::validate_product_response(
                &waiter.form,
                &response,
            )
            .is_ok() =>
        {
            crate::server_requests::ServerRequestAnswer::Result(value)
        }
        Ok(_) => crate::server_requests::ServerRequestAnswer::Invalid(
            "question response does not match the requested schema".to_string(),
        ),
        Err(_) => crate::server_requests::ServerRequestAnswer::Invalid(
            "invalid question response".to_string(),
        ),
    }
}

pub(super) fn set_question_result(
    inner: &mut ServerRequestRuntimeInner,
    request_id: &RequestId,
    value: &serde_json::Value,
) -> bool {
    let Some(waiter) = inner.question_waiters.get_mut(request_id) else {
        return false;
    };
    let Ok(response) = serde_json::from_value(value.clone()) else {
        return false;
    };
    waiter.result = Some(response);
    true
}
