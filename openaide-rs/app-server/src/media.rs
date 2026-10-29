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

/// Width and height an image displays at, read from its header without decoding
/// pixels. EXIF orientation is applied, as browsers do, so a portrait photo
/// stored sideways reports its portrait size. `None` for an unreadable header.
pub(crate) fn display_dimensions(
    reader: impl std::io::BufRead + std::io::Seek,
) -> Option<(u32, u32)> {
    use image::ImageDecoder;

    let mut decoder = image::ImageReader::new(reader)
        .with_guessed_format()
        .ok()?
        .into_decoder()
        .ok()?;
    let (width, height) = decoder.dimensions();
    if width == 0 || height == 0 {
        return None;
    }
    let turned = matches!(
        decoder.orientation().ok()?,
        image::metadata::Orientation::Rotate90
            | image::metadata::Orientation::Rotate270
            | image::metadata::Orientation::Rotate90FlipH
            | image::metadata::Orientation::Rotate270FlipH
    );
    Some(if turned {
        (height, width)
    } else {
        (width, height)
    })
}

#[cfg(test)]
#[path = "media_tests.rs"]
pub(crate) mod tests;
