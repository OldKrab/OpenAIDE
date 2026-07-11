use crate::protocol::model::IsolationKind;

pub struct StartTask {
    pub title: String,
    pub workspace_root: String,
    pub agent_id: String,
    pub isolation: IsolationKind,
    pub prompt_text: String,
    pub model_id: Option<String>,
}
