use std::io::{Cursor, Read};
use std::sync::Mutex;
use std::time::Instant;

use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits};
use openaide_app_server_protocol::file_viewer::FileViewerError;

// Source and decoded budgets protect the server; output budgets protect the protocol and browser.
const INPUT_MAX_BYTES: u64 = 64 * 1024 * 1024;
const DECODE_MAX_BYTES: u64 = 128 * 1024 * 1024;
const SOURCE_MAX_PIXELS: u64 = 32_000_000;
const PREVIEW_MAX_EDGE: u32 = 2048;
const PREVIEW_MAX_BYTES: usize = 2 * 1024 * 1024;

// Admission is independent of the protocol lock, bounding conversion memory across clients.
pub(super) static IMAGE_PREVIEW_GATE: Mutex<()> = Mutex::new(());

pub(super) struct ImagePreview {
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
    pub reduced: bool,
}

/// Produces presentation bytes only. The viewer capability continues to identify the original file.
pub(super) fn load(
    file: std::fs::File,
    prefix: Vec<u8>,
    source_size: u64,
    media_type: &'static str,
) -> Result<ImagePreview, FileViewerError> {
    let started = Instant::now();
    let operation_id = uuid::Uuid::new_v4().to_string();
    crate::logging::info(
        "file_viewer_image_preview_started",
        serde_json::json!({
            "operation_id": operation_id, "attempt": 1, "source_bytes": source_size,
        }),
    );
    let result = match IMAGE_PREVIEW_GATE.lock() {
        Ok(_permit) => {
            crate::logging::info(
                "file_viewer_image_preview_admitted",
                serde_json::json!({
                    "operation_id": operation_id, "wait_ms": started.elapsed().as_millis(),
                }),
            );
            load_inner(file, prefix, source_size, media_type)
        }
        Err(_) => Err(FileViewerError::Unreadable),
    };
    crate::logging::info(
        "file_viewer_image_preview_completed",
        serde_json::json!({
            "operation_id": operation_id, "attempt": 1,
            "duration_ms": started.elapsed().as_millis(),
            "outcome": if result.is_ok() { "success" } else { "failure" },
            "error_kind": result.as_ref().err(),
            "preview_bytes": result.as_ref().ok().map(|preview| preview.bytes.len()),
            "reduced": result.as_ref().ok().map(|preview| preview.reduced),
        }),
    );
    result
}

fn load_inner(
    file: std::fs::File,
    mut bytes: Vec<u8>,
    source_size: u64,
    media_type: &'static str,
) -> Result<ImagePreview, FileViewerError> {
    if source_size > INPUT_MAX_BYTES {
        return Err(FileViewerError::Unsupported);
    }
    // Bound the actual read too: the file can grow after metadata was read.
    file.take(INPUT_MAX_BYTES + 1 - bytes.len() as u64)
        .read_to_end(&mut bytes)
        .map_err(|_| FileViewerError::Unreadable)?;
    if bytes.len() as u64 > INPUT_MAX_BYTES {
        return Err(FileViewerError::Unsupported);
    }
    let format = ImageFormat::from_mime_type(media_type).ok_or(FileViewerError::Unsupported)?;
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(DECODE_MAX_BYTES);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(image_error)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > SOURCE_MAX_PIXELS {
        return Err(FileViewerError::Unsupported);
    }
    // ImageReader::into_decoder does not reserve the output buffer as decode() does.
    limits.reserve(decoder.total_bytes()).map_err(image_error)?;
    decoder.set_limits(limits).map_err(image_error)?;
    let orientation = decoder.orientation().map_err(image_error)?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(image_error)?;
    if bytes.len() <= PREVIEW_MAX_BYTES && width <= PREVIEW_MAX_EDGE && height <= PREVIEW_MAX_EDGE {
        // Keep animation, metadata, and exact colors for already-bounded images.
        return Ok(ImagePreview {
            bytes,
            media_type,
            reduced: false,
        });
    }
    decoded.apply_orientation(orientation);
    let mut preview = decoded.thumbnail(
        decoded.width().min(PREVIEW_MAX_EDGE),
        decoded.height().min(PREVIEW_MAX_EDGE),
    );
    drop(decoded);
    drop(bytes);
    loop {
        let mut encoded = Vec::new();
        let media_type = if preview.color().has_alpha() {
            preview
                .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
                .map_err(image_error)?;
            "image/png"
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, 85)
                .encode_image(&preview.to_rgb8())
                .map_err(image_error)?;
            "image/jpeg"
        };
        if encoded.len() <= PREVIEW_MAX_BYTES {
            return Ok(ImagePreview {
                bytes: encoded,
                media_type,
                reduced: true,
            });
        }
        // Halving dimensions guarantees termination and retains transparency rather than flattening it.
        preview = preview.thumbnail((preview.width() / 2).max(1), (preview.height() / 2).max(1));
    }
}

fn image_error(error: image::ImageError) -> FileViewerError {
    match error {
        image::ImageError::IoError(_) => FileViewerError::Unreadable,
        _ => FileViewerError::Unsupported,
    }
}
