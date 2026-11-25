use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};

use super::*;

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

/// Noise compresses badly, like the detail in a real screenshot.
fn noisy_screenshot(width: u32, height: u32) -> Vec<u8> {
    let mut seed = 0x2545_f491_u32;
    png(&RgbaImage::from_fn(width, height, |_, _| {
        seed ^= seed << 13;
        seed ^= seed >> 17;
        seed ^= seed << 5;
        let [r, g, b, _] = seed.to_le_bytes();
        Rgba([r, g, b, u8::MAX])
    }))
}

#[test]
fn a_large_screenshot_gets_a_smaller_copy_once() {
    let previews = ChatImagePreviews::default();
    let original = noisy_screenshot(1200, 800);

    let first = previews.preview("image-a", &original);
    let ChatPreview::Reduced { media_type, bytes } = &first.preview else {
        panic!("expected a smaller copy, got {first:?}");
    };
    assert_eq!(*media_type, "image/jpeg");
    assert!(bytes.len() < original.len());
    assert!(!first.cache_hit);
    assert_eq!(first.fallback_reason, None);

    let second = previews.preview("image-a", &original);
    assert!(second.cache_hit);
    assert_eq!(second.preview, first.preview);
}

#[test]
fn a_copy_that_would_not_be_smaller_is_not_sent() {
    // A heavily compressed JPEG just over the edge bound: a preview at the preview
    // quality would be larger than the stored bytes.
    let pixels = image::RgbImage::from_fn(1000, 700, |x, y| {
        image::Rgb([(x / 4) as u8, (y / 3) as u8, ((x + y) / 7) as u8])
    });
    let mut original = Vec::new();
    image::codecs::jpeg::JpegEncoder::new_with_quality(&mut original, 5)
        .encode_image(&pixels)
        .unwrap();
    let outcome = ChatImagePreviews::default().preview("compressed", &original);
    assert_eq!(outcome.preview, ChatPreview::Original);
    assert_eq!(outcome.fallback_reason, None);
}

#[test]
fn an_image_previews_cannot_decode_is_shown_as_stored() {
    let outcome =
        ChatImagePreviews::default().preview("svg", b"<svg xmlns='http://www.w3.org/2000/svg'/>");
    assert_eq!(outcome.preview, ChatPreview::Original);
    assert_eq!(outcome.fallback_reason, Some("unknown_format"));
}

#[test]
fn the_cache_forgets_the_least_recently_used_preview_first() {
    let previews = ChatImagePreviews::default();
    let large = |id: u8| ChatPreview::Reduced {
        media_type: "image/jpeg",
        bytes: vec![id; CACHE_MAX_BYTES / 3].into(),
    };
    previews.remember("one", large(1), None);
    previews.remember("two", large(2), None);
    previews.remember("three", large(3), None);
    // Showing "one" again makes "two" the least recently used.
    assert!(previews.cached("one").is_some());
    previews.remember("four", large(4), None);
    assert!(previews.cached("two").is_none());
    assert!(previews.cached("one").is_some());
    assert!(previews.cached("four").is_some());
    assert!(previews.held_bytes() <= CACHE_MAX_BYTES);
}
