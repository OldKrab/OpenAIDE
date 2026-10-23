//! The held read of Task Attention Events for shells without a live Frontend.

use super::{ProtocolMethod, TASK_ATTENTION_WAIT};
use crate::task::{TaskAttentionWaitParams, TaskAttentionWaitResult};

protocol_method!(
    TaskAttentionWait,
    TASK_ATTENTION_WAIT,
    TaskAttentionWaitParams,
    TaskAttentionWaitResult
);
