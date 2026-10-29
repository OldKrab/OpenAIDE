use std::io::Cursor;

use image::{ImageEncoder, RgbImage};

use super::*;

/// An encoded `width`×`height` image, as a phone or an Agent would send it.
pub(crate) fn encoded_png(width: u32, height: u32) -> Vec<u8> {
    let mut bytes = Vec::new();
    image::codecs::png::PngEncoder::new(&mut bytes)
        .write_image(
            RgbImage::new(width, height).as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    bytes
}

/// A JPEG whose pixels are `width`×`height` but whose EXIF says to rotate it a
/// quarter turn, as a phone camera stores a portrait photo.
fn sideways_jpeg(width: u32, height: u32) -> Vec<u8> {
    // Little-endian TIFF with one IFD entry: Orientation (0x0112) = 6, rotate 90° clockwise.
    let exif = vec![
        b'I', b'I', 0x2a, 0x00, 0x08, 0x00, 0x00, 0x00, 0x01, 0x00, 0x12, 0x01, 0x03, 0x00, 0x01,
        0x00, 0x00, 0x00, 0x06, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00, 0x00,
    ];
    let mut bytes = Vec::new();
    let mut encoder = image::codecs::jpeg::JpegEncoder::new(&mut bytes);
    encoder.set_exif_metadata(exif).unwrap();
    encoder
        .write_image(
            RgbImage::new(width, height).as_raw(),
            width,
            height,
            image::ExtendedColorType::Rgb8,
        )
        .unwrap();
    bytes
}

#[test]
fn an_image_reports_the_size_it_displays_at() {
    assert_eq!(
        display_dimensions(Cursor::new(encoded_png(3, 2))),
        Some((3, 2))
    );
}

#[test]
fn a_rotated_photo_reports_its_upright_size() {
    assert_eq!(
        display_dimensions(Cursor::new(sideways_jpeg(3, 2))),
        Some((2, 3))
    );
}

#[test]
fn bytes_that_are_not_an_image_have_no_size() {
    assert_eq!(display_dimensions(Cursor::new(b"image".to_vec())), None);
}
