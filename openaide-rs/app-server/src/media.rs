use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum MediaDataError {
    Invalid,
    TooLarge,
}

/// Validates an inline image before it is persisted or projected into a data URL.
pub(crate) fn validate_base64_image(
    media_type: &str,
    data: &str,
    max_bytes: usize,
) -> Result<usize, MediaDataError> {
    validate_base64_media(media_type, "image/", data, max_bytes)
}

pub(crate) fn validate_base64_audio(
    media_type: &str,
    data: &str,
    max_bytes: usize,
) -> Result<usize, MediaDataError> {
    validate_base64_media(media_type, "audio/", data, max_bytes)
}

fn validate_base64_media(
    media_type: &str,
    expected_prefix: &str,
    data: &str,
    max_bytes: usize,
) -> Result<usize, MediaDataError> {
    if !media_type.starts_with(expected_prefix) || media_type.trim().len() != media_type.len() {
        return Err(MediaDataError::Invalid);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| MediaDataError::Invalid)?;
    if decoded.is_empty() {
        return Err(MediaDataError::Invalid);
    }
    if decoded.len() > max_bytes {
        return Err(MediaDataError::TooLarge);
    }
    Ok(decoded.len())
}
