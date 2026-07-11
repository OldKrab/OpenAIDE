use crate::protocol::errors::RuntimeError;

pub fn from_sequence(sequence: u64) -> String {
    format!("m:{sequence}")
}

pub fn to_sequence(cursor: &str) -> Result<u64, RuntimeError> {
    cursor
        .strip_prefix("m:")
        .ok_or_else(|| RuntimeError::InvalidParams("before_cursor".to_string()))?
        .parse::<u64>()
        .map_err(|_| RuntimeError::InvalidParams("before_cursor".to_string()))
}
