use std::io::Cursor;

use image::{ImageFormat, Rgba, RgbaImage};

use super::*;

const SMALL: PreviewBounds = PreviewBounds {
    max_edge: 64,
    max_bytes: 64 * 1024,
    jpeg_quality: 80,
};

fn png(image: &RgbaImage) -> Vec<u8> {
    let mut bytes = Vec::new();
    image
        .write_to(&mut Cursor::new(&mut bytes), ImageFormat::Png)
        .unwrap();
    bytes
}

#[test]
fn an_opaque_image_with_an_alpha_channel_becomes_a_jpeg() {
    let source = png(&RgbaImage::from_pixel(
        200,
        100,
        Rgba([10, 20, 30, u8::MAX]),
    ));
    let preview = reduce(source, ImageFormat::Png, "image/png", SMALL).unwrap();
    assert!(preview.reduced);
    assert_eq!(preview.media_type, "image/jpeg");
    let shown = image::load_from_memory(&preview.bytes).unwrap();
    assert_eq!((shown.width(), shown.height()), (64, 32));
}

#[test]
fn visible_transparency_stays_png() {
    let mut pixels = RgbaImage::from_pixel(200, 100, Rgba([10, 20, 30, u8::MAX]));
    // A transparent block large enough to survive the reduction.
    for x in 90..110 {
        for y in 40..60 {
            pixels.put_pixel(x, y, Rgba([0, 0, 0, 0]));
        }
    }
    let preview = reduce(png(&pixels), ImageFormat::Png, "image/png", SMALL).unwrap();
    assert!(preview.reduced);
    assert_eq!(preview.media_type, "image/png");
}

#[test]
fn an_image_within_bounds_is_returned_unchanged() {
    let source = png(&RgbaImage::from_pixel(32, 16, Rgba([1, 2, 3, 4])));
    let preview = reduce(source.clone(), ImageFormat::Png, "image/png", SMALL).unwrap();
    assert!(!preview.reduced);
    assert_eq!(preview.media_type, "image/png");
    assert_eq!(preview.bytes, source);
}

#[test]
fn bytes_that_are_not_an_image_are_unsupported() {
    assert_eq!(
        reduce(
            b"not an image".to_vec(),
            ImageFormat::Png,
            "image/png",
            SMALL
        )
        .err(),
        Some(Unsupported)
    );
    assert_eq!(sniff(b"not an image"), None);
    assert_eq!(
        sniff(&png(&RgbaImage::new(1, 1))).map(|(_, media_type)| media_type),
        Some("image/png")
    );
}
