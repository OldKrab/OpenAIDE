use base64::Engine;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ImageDataError {
    Invalid,
    TooLarge,
}

/// Validates an inline image before it is persisted or projected into a data URL.
pub(crate) fn validate_base64_image(
    media_type: &str,
    data: &str,
    max_bytes: usize,
) -> Result<usize, ImageDataError> {
    if !media_type.starts_with("image/") || media_type.trim().len() != media_type.len() {
        return Err(ImageDataError::Invalid);
    }
    let decoded = base64::engine::general_purpose::STANDARD
        .decode(data)
        .map_err(|_| ImageDataError::Invalid)?;
    if decoded.is_empty() {
        return Err(ImageDataError::Invalid);
    }
    if decoded.len() > max_bytes {
        return Err(ImageDataError::TooLarge);
    }
    Ok(decoded.len())
}
