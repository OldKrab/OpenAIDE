//! Typed compatibility seam for ACP elicitation until the runtime SDK exposes preview dispatch.

use std::collections::BTreeMap;

use agent_client_protocol::{JsonRpcNotification, JsonRpcRequest, JsonRpcResponse};
use serde::{Deserialize, Serialize};

#[cfg(test)]
#[path = "acp_elicitation_wire_tests.rs"]
mod tests;

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcRequest)]
#[request(method = "elicitation/create", response = ElicitationCreateResponse)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ElicitationCreateRequest {
    #[serde(default)]
    pub(super) session_id: Option<String>,
    #[serde(default)]
    pub(super) request_id: Option<WireRequestId>,
    #[serde(default)]
    pub(super) tool_call_id: Option<String>,
    pub(super) mode: ElicitationMode,
    pub(super) message: String,
    #[serde(default)]
    pub(super) requested_schema: Option<ElicitationSchema>,
    #[serde(default, rename = "_meta")]
    pub(super) meta: Option<serde_json::Map<String, serde_json::Value>>,
}

#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq, Hash)]
#[serde(untagged)]
pub(super) enum WireRequestId {
    String(String),
    Integer(i64),
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub(super) enum ElicitationMode {
    Form,
    Url,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct ElicitationSchema {
    #[serde(rename = "type")]
    pub(super) type_: ObjectType,
    #[serde(default)]
    pub(super) title: Option<String>,
    #[serde(default)]
    pub(super) description: Option<String>,
    #[serde(default)]
    pub(super) properties: BTreeMap<String, PropertySchema>,
    #[serde(default)]
    pub(super) required: Vec<String>,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum ObjectType {
    Object,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type", rename_all = "snake_case", deny_unknown_fields)]
pub(super) enum PropertySchema {
    String {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default, rename = "minLength")]
        min_length: Option<u32>,
        #[serde(default, rename = "maxLength")]
        max_length: Option<u32>,
        #[serde(default)]
        pattern: Option<String>,
        #[serde(default)]
        format: Option<StringFormat>,
        #[serde(default)]
        default: Option<String>,
        #[serde(default, rename = "enum")]
        enum_values: Option<Vec<String>>,
        #[serde(default, rename = "oneOf")]
        one_of: Option<Vec<EnumOption>>,
    },
    Number {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        minimum: Option<f64>,
        #[serde(default)]
        maximum: Option<f64>,
        #[serde(default)]
        default: Option<f64>,
    },
    Integer {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        minimum: Option<i64>,
        #[serde(default)]
        maximum: Option<i64>,
        #[serde(default)]
        default: Option<i64>,
    },
    Boolean {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default)]
        default: Option<bool>,
    },
    Array {
        #[serde(default)]
        title: Option<String>,
        #[serde(default)]
        description: Option<String>,
        #[serde(default, rename = "minItems")]
        min_items: Option<u64>,
        #[serde(default, rename = "maxItems")]
        max_items: Option<u64>,
        items: MultiSelectItems,
        #[serde(default)]
        default: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "kebab-case")]
pub(super) enum StringFormat {
    Email,
    Uri,
    Date,
    DateTime,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum MultiSelectItems {
    Untitled {
        #[serde(rename = "type")]
        type_: StringType,
        #[serde(rename = "enum")]
        values: Vec<String>,
    },
    Titled {
        #[serde(rename = "anyOf")]
        options: Vec<EnumOption>,
    },
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum StringType {
    String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(super) struct EnumOption {
    #[serde(rename = "const")]
    pub(super) value: String,
    pub(super) title: String,
    #[serde(default)]
    pub(super) description: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcResponse)]
#[serde(tag = "action", rename_all = "snake_case")]
pub(super) enum ElicitationCreateResponse {
    Accept {
        content: BTreeMap<String, ElicitationContentValue>,
    },
    Cancel,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(untagged)]
pub(super) enum ElicitationContentValue {
    String(String),
    Integer(i64),
    Number(f64),
    Boolean(bool),
    StringArray(Vec<String>),
}

#[derive(Debug, Clone, Deserialize, Serialize, JsonRpcNotification)]
#[notification(method = "$/cancel_request")]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(super) struct CancelRequestNotification {
    pub(super) request_id: WireRequestId,
    #[serde(default, rename = "_meta")]
    pub(super) meta: Option<serde_json::Map<String, serde_json::Value>>,
}
