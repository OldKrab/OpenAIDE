use std::path::Path;

use serde::{Deserialize, Serialize};

#[derive(Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct UpdateAttemptReceipt {
    pub(crate) previous_version: String,
    pub(crate) target_version: String,
    pub(crate) attempted_at_ms: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub(crate) enum ReceiptOutcome {
    None,
    Updated(String),
    Incomplete,
}

pub(crate) fn classify_receipt(
    current_version: &str,
    receipt: Option<&UpdateAttemptReceipt>,
) -> ReceiptOutcome {
    let Some(receipt) = receipt else {
        return ReceiptOutcome::None;
    };
    if current_version == receipt.target_version {
        ReceiptOutcome::Updated(receipt.target_version.clone())
    } else {
        ReceiptOutcome::Incomplete
    }
}

pub(crate) fn write_receipt(path: &Path, receipt: &UpdateAttemptReceipt) -> Result<(), String> {
    let parent = path
        .parent()
        .ok_or_else(|| "Desktop Update receipt location is unavailable.".to_string())?;
    std::fs::create_dir_all(parent)
        .map_err(|_| "Desktop Update could not prepare its receipt.".to_string())?;
    let temporary = path.with_extension("json.tmp");
    let bytes = serde_json::to_vec(receipt)
        .map_err(|_| "Desktop Update could not encode its receipt.".to_string())?;
    std::fs::write(&temporary, bytes)
        .map_err(|_| "Desktop Update could not save its receipt.".to_string())?;
    std::fs::rename(temporary, path)
        .map_err(|_| "Desktop Update could not commit its receipt.".to_string())
}

pub(crate) fn read_receipt(path: &Path) -> Option<UpdateAttemptReceipt> {
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
}
