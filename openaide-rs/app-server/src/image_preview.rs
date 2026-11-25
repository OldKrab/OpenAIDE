//! Smaller copies of raster images for presentation: File Viewer image previews and
//! Chat image previews (ADR-0084). A preview is presentation bytes only; callers keep
//! identifying the original file or stored image.

use std::io::Cursor;
use std::sync::Mutex;

use image::{DynamicImage, GenericImageView, ImageDecoder, ImageFormat, ImageReader, Limits};

// Source and decoded budgets protect the server from a hostile or huge file.
pub(crate) const INPUT_MAX_BYTES: u64 = 64 * 1024 * 1024;
const DECODE_MAX_BYTES: u64 = 128 * 1024 * 1024;
const SOURCE_MAX_PIXELS: u64 = 32_000_000;

/// Admits one decode at a time across every caller and client, which bounds the
/// memory conversions hold at once. It is independent of the protocol lock.
pub(crate) static GATE: Mutex<()> = Mutex::new(());

/// What a preview may be. An image already within both bounds is returned as is.
#[derive(Debug, Clone, Copy)]
pub(crate) struct PreviewBounds {
    /// The longest edge in pixels, after EXIF orientation.
    pub max_edge: u32,
    /// The most encoded bytes; a larger encoding is halved until it fits.
    pub max_bytes: usize,
    pub jpeg_quality: u8,
}

pub(crate) struct ImagePreview {
    pub media_type: &'static str,
    pub bytes: Vec<u8>,
    /// False when `bytes` are the source bytes unchanged.
    pub reduced: bool,
}

/// The source cannot become a preview: an unknown or damaged format, or one over the
/// source budgets. Callers decide whether to show the original instead.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct Unsupported;

/// Decodes `bytes` of `format` and returns a copy within `bounds`. Opaque images
/// become JPEG; images with visible transparency stay PNG, so transparency is never
/// flattened onto a color the theme did not choose.
pub(crate) fn reduce(
    bytes: Vec<u8>,
    format: ImageFormat,
    media_type: &'static str,
    bounds: PreviewBounds,
) -> Result<ImagePreview, Unsupported> {
    if bytes.len() as u64 > INPUT_MAX_BYTES {
        return Err(Unsupported);
    }
    let mut reader = ImageReader::with_format(Cursor::new(&bytes), format);
    let mut limits = Limits::default();
    limits.max_image_width = Some(16_384);
    limits.max_image_height = Some(16_384);
    limits.max_alloc = Some(DECODE_MAX_BYTES);
    reader.limits(limits.clone());
    let mut decoder = reader.into_decoder().map_err(|_| Unsupported)?;
    let (width, height) = decoder.dimensions();
    if u64::from(width) * u64::from(height) > SOURCE_MAX_PIXELS {
        return Err(Unsupported);
    }
    // ImageReader::into_decoder does not reserve the output buffer as decode() does.
    limits
        .reserve(decoder.total_bytes())
        .map_err(|_| Unsupported)?;
    decoder.set_limits(limits).map_err(|_| Unsupported)?;
    let orientation = decoder.orientation().map_err(|_| Unsupported)?;
    let mut decoded = DynamicImage::from_decoder(decoder).map_err(|_| Unsupported)?;
    if bytes.len() <= bounds.max_bytes && width <= bounds.max_edge && height <= bounds.max_edge {
        // Keep animation, metadata, and exact colors for already-bounded images.
        return Ok(ImagePreview {
            bytes,
            media_type,
            reduced: false,
        });
    }
    decoded.apply_orientation(orientation);
    let mut preview = decoded.thumbnail(
        decoded.width().min(bounds.max_edge),
        decoded.height().min(bounds.max_edge),
    );
    drop(decoded);
    drop(bytes);
    // Screenshots often carry an alpha channel that no pixel uses.
    let transparent =
        preview.color().has_alpha() && preview.pixels().any(|(_, _, pixel)| pixel.0[3] != u8::MAX);
    loop {
        let mut encoded = Vec::new();
        let media_type = if transparent {
            preview
                .write_to(&mut Cursor::new(&mut encoded), ImageFormat::Png)
                .map_err(|_| Unsupported)?;
            "image/png"
        } else {
            image::codecs::jpeg::JpegEncoder::new_with_quality(&mut encoded, bounds.jpeg_quality)
                .encode_image(&preview.to_rgb8())
                .map_err(|_| Unsupported)?;
            "image/jpeg"
        };
        if encoded.len() <= bounds.max_bytes {
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

/// The raster format and media type of `bytes`, from their signature, for the formats
/// previews decode.
pub(crate) fn sniff(bytes: &[u8]) -> Option<(ImageFormat, &'static str)> {
    if bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some((ImageFormat::Png, "image/png"))
    } else if bytes.starts_with(b"\xff\xd8\xff") {
        Some((ImageFormat::Jpeg, "image/jpeg"))
    } else if bytes.starts_with(b"GIF87a") || bytes.starts_with(b"GIF89a") {
        Some((ImageFormat::Gif, "image/gif"))
    } else if bytes.len() >= 12 && bytes.starts_with(b"RIFF") && &bytes[8..12] == b"WEBP" {
        Some((ImageFormat::WebP, "image/webp"))
    } else {
        None
    }
}

#[cfg(test)]
#[path = "image_preview_tests.rs"]
mod tests;
