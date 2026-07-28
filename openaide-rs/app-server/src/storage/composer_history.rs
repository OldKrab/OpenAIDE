use serde::{Deserialize, Serialize};

const HISTORY_LIMIT: usize = 50;

/// Durable text accepted through OpenAIDE, independent from replaceable Native Session Chat.
#[derive(Debug, Clone, Deserialize, Serialize, PartialEq, Eq)]
pub struct ComposerHistoryEntryRecord {
    pub entry_id: String,
    pub project_id: String,
    pub text: String,
    pub accepted_at: String,
}

#[derive(Debug, Clone, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(transparent)]
pub struct ComposerHistory(Vec<ComposerHistoryEntryRecord>);

impl ComposerHistory {
    /// Records one accepted text value, moving an exact duplicate to the newest position.
    pub fn record(&mut self, incoming: ComposerHistoryEntryRecord) {
        if let Some(index) = self.0.iter().position(|entry| entry.text == incoming.text) {
            let mut existing = self.0.remove(index);
            existing.project_id = incoming.project_id;
            existing.accepted_at = incoming.accepted_at;
            self.0.insert(0, existing);
        } else {
            self.0.insert(0, incoming);
            self.0.truncate(HISTORY_LIMIT);
        }
    }

    pub fn entries(&self) -> &[ComposerHistoryEntryRecord] {
        &self.0
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

#[cfg(test)]
#[path = "composer_history_tests.rs"]
mod tests;
