use std::io::Read;
use std::time::Instant;

use image::ImageFormat;
use openaide_app_server_protocol::file_viewer::FileViewerError;

use crate::image_preview::{self, ImagePreview, PreviewBounds, INPUT_MAX_BYTES};

// Output budgets protect the protocol and the browser.
const BOUNDS: PreviewBounds = PreviewBounds {
    max_edge: 2048,
    max_bytes: 2 * 1024 * 1024,
    jpeg_quality: 85,
};

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
    let result = match image_preview::GATE.lock() {
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
    image_preview::reduce(bytes, format, media_type, BOUNDS)
        .map_err(|image_preview::Unsupported| FileViewerError::Unsupported)
}
